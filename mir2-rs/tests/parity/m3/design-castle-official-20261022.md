# Lane castleofficial — 攻城 NPC 标签设计（`CastleOfficial.UserSelect` 全标签）

- 仓库根：`/e/Users/gxh/Documents/GitHub/OpenMir2`；基线 `master = c83a9ae24`；本文档分支 `castle-official-design`。
- 车道性质：**tier=L，纯文档 + 只读**。本文是本车道唯一改动；未改 `src/**`、`sql/**`、`MirClient/`、
  `mir2-rs/crates/{protocol,storage}`、`E:/MirServer/**`；未起服务端/客户端/代理，未占端口，未写数据库，
  未动主检出，未 push/合并。
- 输入：`mir2-rs/tests/parity/m3/audit-uncovered-surfaces-20261019.md` #6（P0）与评审批注 D-2/D-4；
  `M3⑧公会攻城现状盘点报告-20261013.md` §1.L（N1/N5）；白名单 `T-38`/`T-40`。
- oracle（只读）：`src/GameSrv/Npc/CastleOfficial.cs`、`src/GameSrv/Npc/Merchant.cs`、`src/M2Server/Npc/NormNpc.cs`、
  `src/M2Server/Castle/{UserCastle.cs,CastleManager.cs,CastleConf.cs,CastleConst.cs}`、
  `src/M2Server/Monster/Monsters/CastleDoor.cs`、`src/M2Server/Actor/{BaseObject.Message.cs,BaseObject.cs}`、
  `src/M2Server/Player/{PlayObject.Base.cs,PlayObject.Operate.cs,PlayObject.Message.cs,PlayObject.cs}`、
  `src/Modules/ScriptEngine/{ScriptEngine.cs,ScriptParsers.cs}`、
  `src/Modules/SystemModule/Const/ScriptFlagConst.cs`、`src/OpenMir2/Messages.cs`。
- Rust（只读）：`mir2-rs/crates/world/src/{castle.rs,castle_tax.rs}`、`mir2-rs/crates/data/src/guild_castle.rs`、
  `mir2-rs/crates/envir/src/castle.rs`、`mir2-rs/crates/world/src/npc.rs`、
  `mir2-rs/bins/game-svr/src/{npcs.rs,npc.rs,shop.rs,castle.rs,main.rs,smframe.rs}`。
- 部署件（只读，未写）：`E:/MirServer/M2GameSvr/Envir/{Npcs.txt,Npc_def/沙巴克城堡管理-0150.txt}`、
  `Castle/{Castle,0}/SabukW.txt`、`Castle/List.txt`、`Server.conf`、`sql/mir2_data.sql`。
- 口径：**行为等价优先于修 bug**。本文把「已实现」「已登记（N1/N5）」「部署不可达」「Rust 缺口」四类分开写；
  不把既有缺口当新发现，也不把 oracle 的怪行为当 bug 修。

---

## 0. 摘要与关键结论

1. **audit #6 的「P0 生产可达」需要按层更正（本节是全文最重要的结论）**：
   - **普通客户端全链不可达**。`CastleOfficial.Click` 的门是 `Castle.IsMasterGuild(MyGuild) || Permission >= 3`
     （`CastleOfficial.cs:27`）。部署件满足：① `OwnGuild=` 为空且 `MasterGuild = GuildMgr.FindGuild(OwnGuild)`
     在 `CastleMgr.Initialize`（`GameApp.cs:213`）里执行，早于 `GuildMgr.LoadGuildInfo`（`GameApp.cs:245`）
     ⇒ **`MasterGuild` 永远为 null**；② `PlayObject.Permission` 生产恒 `0`
     （`GetHumPermission` 的唯一调用点被注释，`PlayObject.cs:1188-1196`；`Server.conf:365 StartPermission=0`；例外：DEBUG 构建 `GameCommand.cs:32-35` 会给执行命令者置 10，stock Release 不成立）。
     ⇒ 正常点 NPC 对任何玩家都是**静默无反应**（不是显示对话）。
   - **但 say / 变量 / 脚本标签层可被「构造包」直接触达**：`CM_MERCHANTDLGSELECT(1011)` 的入口
     `ClientMerchantDlgSelect`（`PlayObject.Operate.cs:114-127`）**不检查 Click、不检查权限、不检查城主**，
     只查 `FindMerchant ?? FindNpc` + 同图/±15 视距。`CastleOfficial.UserSelect` 第 97 行先调
     `base.UserSelect`（= `Merchant.UserSelect`），而 `CastleMerchant=false`（`Npcs.txt` 不设该列）
     ⇒ `Merchant.UserSelect` 的门为真，会执行 `GotoLable(@main)` 与脚本标签。**因此 BotSrv/自造客户端
     直发 1011 就能拿到 `@main`、`@treatdoor`、`@repairdoor`、`@hirearchers` 等说词帧**，
     无需 GM 权限。这是本面唯一「真机可对拍」的入口。
   - **城堡状态动作（开门/关门/修门/修墙/雇佣/改名/金库）在 oracle 里不可达**：它们全在
     `Castle.IsMasterGuild(MyGuild) && PlayObject.IsGuildMaster()`（`CastleOfficial.cs:110`）之内，
     而 `MasterGuild` 恒 null（R-0，见 §1.2）。`Permission>=3` 也补不上（该门不认 Permission，
     且 Permission 生产恒 0）。⇒ 这部分只能**构造级注入**验证，真机窗口不能靠「正常玩法」覆盖。
2. **两个已登记 oracle 缺陷必须先照搬，不许「顺手修」**：
   - **N1（T-38）**：`@@withdrawal`/`@@receipts` 调的是 `IUserCastle` 接口重载
     `WithDrawalGolds(IPlayerActor,int)`/`ReceiptGolds(IPlayerActor,int)`（`UserCastle.cs:599-602/911-914`），
     两者 `throw new NotImplementedException`，被 `UserSelect` 的 `catch` 吞掉 ⇒ **静默无效果**
     （无帧、无金币变动、无提示）。
   - **N5（T-40）**：`@hireguardnow*`/`@hirearchernow*` 的
     `sLabel.Substring(prefix.Length, sLabel.Length)`（`CastleOfficial.cs:229/235`）第二参用整串长度
     ⇒ 恒抛 `ArgumentOutOfRangeException` ⇒ 被吞 ⇒ 雇佣**不可达**。附加一层：即使有人「修好」Substring，
     `@hirearchernow12` 的 `n10=12` 仍会命中 `Archers[12]` 越界（数组长 12，比较是 `n10 <= 12`），
     `@hirearchernow0` 的 `n10=-1` 也越界 ⇒ 该分支是**双重坏**，Rust 不得实现「能雇佣」。
3. **Rust 已有「纯函数半」，缺「宿主半」**：
   - 已有：`door_state_text`（`castle.rs:463`）、`castle_official_action`（`:510`）、`compare_lstr`（`:486`）、
     `hire_substring`（`:538`）、`official_repair_door/wall`（`:553/:589`）、`main_door_control`（`:1398`）、
     `repair_door/wall`（`:1424/:1448`）、N1 no-op（`:1613/:1638`）、门/墙掩码与 `StructureEvent`；
     配置价 `crates/data/src/guild_castle.rs:106-113`（HireGuard/HireArcher/RepairDoor/RepairWall）。
   - 缺：`bins/game-svr` 无 `FindNpc`（`CM_CLICKNPC`/`CM_MERCHANTDLGSELECT` 都只认
     `e.merchant.is_some()`，`main.rs:3771/4371`）、无 `Npc_def` 固定 NPC 脚本装载
     （`npcs.rs` 只放置实体；`npc.rs::NpcHost` 只吃 MapQuest/QFunction）、无 `$CASTLEGOLD` 等
     NPC 侧变量展开（脚本引擎 `line_variable_text` 缺省原样返回，`crates/script/src/engine/mod.rs:131`）、
     无 `CanJmpScriptLableMap` 生产填充、无 `ScriptCurrLable/ScriptGoBackLable`、无城堡标签派发、
     无 `SabukW.txt` 写侧、无 `MsgQueue` 优先级出队模型。
4. **部署内容决定了三条具体现象（必须写进判据）**：
   - `Npcs.txt:4` 的 `沙巴克城堡管理 2 0150 7 16 0 8` 会生成 `CastleOfficial`；`0150` 是
     `SabukW[Defense] CastlePlaceMap` ⇒ `NormNpc.Initialize`（`:1676-1680`）把 `Castle` 设为该城。
   - `Castle/Castle/SabukW.txt` 的门/墙名是 `MainDoor/LeftWall/...`，怪物库无此名
     （`sql/mir2_data.sql` 只有 `沙巴克城门/左/中/右城墙`，`:366-369`）⇒ 启动 **0 个防御实体**
     ⇒ `Castle.MainDoor.BaseObject == null`。于是 `$CASTLEDOORSTATE`（`CastleOfficial.cs:52-67`）
     对 null 无检查 ⇒ `@treatdoor` 页在渲染时抛 `NullReferenceException`，被 `UserSelect` 的
     `catch`（`:258-261`）吞 ⇒ **该页不显示、只落一行常量日志**。这是部署态的真实 oracle 行为。
   - 部署价：`HireGuard=300000`、`HireArcher=300000`、`RepairDoor=2000000`、`RepairWall=500000`
     （`Server.conf:306-309`）；`SubkMasterSendMsg=1`（`:817`）；`ShowPreFixMsg=0`（`:778`）；
     `RedMsgFColor/BColor=255/56`、`GreenMsgFColor/BColor=219/255`（`:600-605`）。
5. **帧面**：本面涉及的 ident 与出队优先级（`MsgQueue.Enqueue(..., (byte)wIdent)`，
   `PriorityQueue` 升序出队）必须一起建模，否则帧序对不上：
   - `SM_MERCHANTSAY(643)` ← `RM_MERCHANTSAY(11009)`，优先级 **1**
   - `SM_MENU_OK(767)` ← `RM_MENU_OK(10309)`，优先级 **69**
   - `SM_MERCHANTDLGCLOSE(644)` ← `RM_MERCHANTDLGCLOSE(10127)`，优先级 **143**
   - `SM_USERNAME(42)` ← `RM_USERNAME(8101)`，优先级 **165**
   - `SM_ALIVE(27)` ← `RM_ALIVE(10153)`，优先级 **169**
   - `SM_DIGUP(20)` ← `RM_DIGUP(10200)`，优先级 **216**；`SM_DIGDOWN(21)` ← `RM_DIGDOWN(10201)`，**217**
   - `SM_SYSMESSAGE(100)` ← `RM_SYSMESSAGE(11004)`，优先级 **252**
   同优先级之间 .NET `PriorityQueue` **不保证稳定序**（`@exit` 的双 close 帧只能按多重集断言）。
6. **判级**：本文档 L。实现卡整体 **R**（`repair` 的 60s 严格 `>` 时序、门帧/`SendRefMsg` 500ms 缓存、
   `@@castlename` 文件持久化、帧出队序）；其中「标签映射/变量文本/N1/N5 no-op/门状态纯函数」可拆 L 子卡。
7. **建议拆三卡并串行**（写集见 §6）：A=纯层（`crates/world`，L）；B=固定 NPC 对话宿主（`bins/game-svr`，L/R，
   **与 guildofficial 争同一写集，必须等其让出**）；C=状态动作 + 持久化（R，且被 R-0 挡在真机之外）。

---

## 1. 可达性（以部署件为准）

### 1.1 五条路径

| # | 路径 | 触发 | oracle 门 | 可达性 | 能观测到什么 |
| --- | --- | --- | --- | --- | --- |
| P1 | 正常点击 NPC | `CM_CLICKNPC(1010)` → `ClientClickNpc` → `FindMerchant ?? FindNpc` → `Click` | `Castle != null` + (`IsMasterGuild` 或 `Permission>=3`) | **不可达**（R-0：MasterGuild null；Permission 恒 0） | 无帧、无日志 |
| P2 | 构造包直发对话选择 | `CM_MERCHANTDLGSELECT(1011)` → `ClientMerchantDlgSelect` → `npc.UserSelect` | 仅 `Death/Ghost` + `FindMerchant ?? FindNpc` + 同图/±15（或 `IsHide`） | **可达（BotSrv/自造客户端）** | `Merchant.UserSelect` 的 `@main`/脚本页说词帧 + `$` 变量展开 + 标签表 |
| P3 | P2 后点脚本标签 | 1011，label = `@treatdoor`/`@repairdoor` 等 | 同 P2；`LableIsCanJmp` 为真（上一页说词里出现过该标签） | **可达** | 脚本 say 帧；`$CASTLEDOORSTATE` 页因 NRE 不显示 |
| P4 | P2 后点城堡动作标签 | 1011，label = `@openmaindoor`/`@repairdoornow`/`@hirearchernowN`/`@@withdrawal`/… | P2 + `IsMasterGuild && IsGuildMaster` | **不可达**（R-0） | oracle 只有 base 的脚本 `#ACT` 部分（若有）与异常日志 |
| P5 | 非 `@` 的 `sData` | 1011，`sData` 不以 `@` 开头 | 无门 | 可达（构造包） | `SM_MENU_OK` 尾「你没有权利使用.」 |

> P4 的「base 脚本 `#ACT` 部分」：`@openmaindoor`/`@closemaindoor` 是脚本标签（`Npc_def` 第 35-44 行），
> 含 `MESSAGEBOX` + `goto @main`。因为 `CastleOfficial.UserSelect` **先调 base**，所以即使城主门为假，
> 这两个标签的脚本动作仍会执行（对任何能发 1011 的人）；**只有 `Castle.MainDoorControl` 不执行**。
> 即：客户端会看到「城门已经打开。」与主页面，但门不动。这是 R-0 下最容易被误判成「已实现」的点。

### 1.2 R-0：部署态无城主（新增可达性结论）

| 事实 | 证据 |
| --- | --- |
| `CastleMgr.Initialize` 早于 `GuildMgr.LoadGuildInfo` | `GameApp.cs:213` vs `:245`（`LoadNpcs` 在 `:229`，所以 NPC 的 `Castle` 仍能解析） |
| `MasterGuild` 的可达赋值只在 `UserCastle.LoadConfig`（`GetCastle(IGuild)` 是 N1，见下一行） | `UserCastle.cs:228-232`；`Initialize` 在 `:111` 调 `LoadConfig` |
| 部署 `OwnGuild` 为空 | `Castle/Castle/SabukW.txt` `[Setup] OwnGuild=` |
| `GetCastle(IGuild)`（改归属）也是 N1 | `UserCastle.cs:644-647`（`throw NotImplementedException`）；`ChangeSabukLordCommand.cs:39` 调接口重载 |
| `Permission` 在 stock Release 恒 0（DEBUG 构建例外） | `PlayObject.cs:1188-1196` 调 `GetHumPermission` 的代码被注释；`Server.conf:365 StartPermission=0`；DEBUG 的 `GameCommand.cs:32-35` 会把执行命令者置 10 |
| 结论 | `IsMasterGuild` 恒 false；`Permission>=3` 也不成立 ⇒ `Click` 静默、`UserSelect` 动作分支永不执行 |

> 评审提示：把 R-0 当成「本面的 N1 第 11 条」登记（与 T-38 同族），但**不要**把它写成「实现差异」——
> Rust 也应让该分支在等价输入下不可达；若宿主将来物化 `Permission`，必须照 oracle 保持 `0`
> （除非另开 `S-*` 修）。

### 1.3 与 audit D-2 的关系

- D-2 说「#6 的 NPC 对话文本可达」——**部分正确**：P2/P3 可达，但 P1 不可达。
- D-2 说「无城主时 oracle 直接返回」——**正确**，但只对 `CastleOfficial` 自己的动作分支；
  `base.UserSelect` 不受该门限制，故 P2 仍能拿说词。本文件以 §1.1 为准。

---

## 2. 执行链（逐行锚）

### 2.1 点击链

```text
CM_CLICKNPC(1010)（PlayObject.Message.cs:885-887）
  → ClientClickNpc(nParam1)（PlayObject.Base.cs:674-694）
      if (!IsCanDeal) { RM_MENU_OK(CanotTryDealMsg); return; }   // IsCanDeal 生产恒真
      if (Death || Ghost) return;
      if (GetTickCount() - ClickNpcTime > Config.ClickNpcTime) {  // 默认 1000ms，严格 >
          ClickNpcTime = GetTickCount();
          normNpc = FindMerchant(actorId) ?? FindNpc(actorId);
          if (normNpc != null && normNpc.Envir == Envir && |dx|<=15 && |dy|<=15)
              normNpc.Click(this);
      }
CastleOfficial.Click（CastleOfficial.cs:20-31）
  if (Castle == null) { SysMsg("NPC不属于城堡!!!", Red, Hint); return; }   // 不可达（部署 Castle 非 null）
  if (Castle.IsMasterGuild(MyGuild) || Permission >= 3) base.Click(this);  // R-0 下恒 false
  // 否则：无任何帧、无消息（不是提示）
Merchant.Click（Merchant.cs:1097-1100）→ NormNpc.Click（NormNpc.cs:68-74）
  ScriptGotoCount=0; ScriptGoBackLable=""; ScriptCurrLable=""; GotoLable(this, "@main", false);
```

### 2.2 对话选择链（P2）

```text
CM_MERCHANTDLGSELECT(1011)（PlayObject.Message.cs:888-890）
  → ClientMerchantDlgSelect(nParam1, sMsg)（PlayObject.Operate.cs:114-127）
      if (Death || Ghost) return;
      npc = FindMerchant(nParam1) ?? FindNpc(nParam1);  if (npc == null) return;
      if (npc.Envir == Envir && IsWithinSight(npc) || npc.IsHide)
          npc.UserSelect(this, sMsg.Trim());
CastleOfficial.UserSelect（CastleOfficial.cs:93-262）
  base.UserSelect(PlayObject, sData);                     // ← Merchant.UserSelect，先执行（关键）
  try {
    if (Castle == null) { SysMsg("NPC不属于城堡!!!"); return; }
    if (sData 以 '@' 开头) {
      sMsg = GetValidStr3(sData, ref sLabel, '\r');        // sLabel = 首段；sMsg = 余下（值）
      PlayObject.ScriptLable = sData;
      if (Castle.IsMasterGuild(MyGuild) && PlayObject.IsGuildMaster()) {   // R-0 下恒 false
        boCanJmp = PlayObject.LableIsCanJmp(sLabel);
        if (sLabel == "@@sendmsg" && sMsg 为空) return;
        GotoLable(PlayObject, ActorId, sLabel, !boCanJmp); // 先执行脚本标签（说词/#ACT）
        if (!boCanJmp) return;                             // 未说过的标签：整段动作跳过
        switch (sLabel) { ... 见 §3 ... }
      }
    } else {
      PlayObject.SendMsg(this, RM_MENU_OK, 0, ActorId, 0, 0, "你没有权利使用.");
    }
  } catch { LogService.Error("[Exception] TCastleManager::UserSelect... "); }  // 常量、无栈、无 data
```

**必须先调 base**：`Merchant.UserSelect`（`Merchant.cs:787-953`）自己也会 `GotoLable`、也会处理
`@exit`/`@back`（`:918-928`）与 `@@sendmsg`（`:820-826`，但要求 `IsSendMsg`）。因此城主动作命中时，
`@exit` 会出**两条** `SM_MERCHANTDLGCLOSE`、`@back` 会出**两份**返回页说词（base 一次 + Castle 一次）。

### 2.3 说词 / 变量 / 标签表链

```text
GotoLable（ScriptEngine.cs:38-129）
  → 执行 sayingRecord.ProcedureList
  → GotoLableSendMerChantSayMsg（ScriptEngine.cs:987-999）
      sMsg = normNpc.GetLineVariableText(playerActor, sMsg);   // NPC 侧变量展开
      playerActor.GetScriptLabel(sMsg);                        // Clear + 重建 CanJmpScriptLableMap
      SendPriorityMsg(RM_MERCHANTSAY, …, ChrName + '/' + sMsg, High) 或 SendMsg(RM_MERCHANTSAY, …, …)
GetLineVariableText（NormNpc.cs:245-272）
  循环：sMsg.IndexOf('>') < 1 ⇒ 停（注意：'>' 在 0 位也停）；ArrestStringEx('<','>') 取 token；
        token[0]=='$' ⇒ GetVariableText（NormNpc → Merchant → CastleOfficial 的 override 链）
        最多 101 次
GetVariableText 链：CastleOfficial.cs:33-91 → Merchant.cs:1102-1129 → NormNpc.cs:275-1585
GetScriptLabel（PlayObject.Base.cs:2775-2807）
  Clear；按 '\\' 分段；每段用正则取 `<...>`；`GetValidStr3(inner, …, '/')` 的**第二段** = 标签
  （`HUtil32.Backslash == '/'`，`HUtil32.cs:12`；`GetValidStr3` 返回第二段、dest=第一段，`:384-389`）
LableIsCanJmp（PlayObject.Base.cs:2814-2826）
  "@main" 恒真；否则查 CanJmpScriptLableMap；否则 PlayDiceLabel
```

> **标签可达性的真实含义**：`CanJmpScriptLableMap` 在**每次说词**时清空重建 ⇒ 只有「最近一页」
> 里出现的链接可点；`@main` 例外。部署 `[@main]` 注册的链接是
> `@@withdrawal`、`@@receipts`、`@treatdoor`、`@repaircastle`、`@hirearchers`、`@commandorder`、
> `@aboutwar`、`@day`；`[@treatdoor]` 注册 `@closemaindoor`、`@openmaindoor`、`@main`；
> `[@repairdoor]` 注册 `@repairdoornow`、`@repaircastle`；`[@repairwalls]` 注册
> `@repairwallnow1/2/3`、`@repaircastle`；`[@hirearchers]` 注册 `@hirearchernow1..12`、`@main`。
> **`@@sendmsg`、`@@offlinemsg`、`@exit`、`@hireguardnow*` 不在任何页面的链接里**
> （`(@@sendmsg …)` 首行是脚本头，不是链接；且 `Npc_def` 走 `LoadScript(..., boFlag=false)`，
> 头处理被跳过 ⇒ `IsSendMsg/IsOffLineMsg` 也不会被置真）。

---

## 3. 逐标签规格（oracle 行号 / 输入 / 副作用 / 帧 / 跳转）

### 3.1 总表

| 标签 | oracle 行 | 输入 | 副作用（状态/文件/广播） | 帧（ident；Recog；尾） | 脚本跳转与顺序 | 部署可达 |
| --- | --- | --- | --- | --- | --- | --- |
| （Click 门） | `CastleOfficial.cs:20-31` | 无 | 无（或在授权时 `GotoLable(@main)`） | 授权时 `SM_MERCHANTSAY(643)` ×页 | `@main` | P1 不可达 |
| `@openmaindoor` | `:199-202`；脚本 `Npc_def:35-39` | 无 | `MainDoorControl(false)`：门未开且非死/非 ghost ⇒ `Open()`（`Dir=7`、`IsOpened=true`、`StoneMode=true`、`HoldPlace=false`、15 条格子掩码） | 脚本 `MESSAGEBOX` → `SM_MENU_OK(767)`（Recog=**玩家** ActorId；尾「城门已经打开。」）+ `@main` say(643)；随后门帧 `SM_DIGUP(20)`（Recog=门 ActorId；Param=x；Tag=y；Series=MakeWord(dir,Light)） | `GotoLable(@openmaindoor)` → `MESSAGEBOX`+`goto @main`；返回后 `MainDoorControl` | P4 不可达；P2+P3 下脚本部分可达（门不动、无 DIGUP） |
| `@closemaindoor` | `:203-206`；脚本 `:40-44` | 无 | `MainDoorControl(true)`：已开且非死/非 ghost ⇒ `Close()`（`Dir=3-Round(HP/MaxHP*3)`，`>=3⇒0`；`IsOpened=false`；`StoneMode=false`；`HoldPlace=true`；12 条掩码） | `SM_MENU_OK` 尾「城门已经关闭。」+ `@main` say；`SM_DIGDOWN(21)`（Series 恒 0，`PlayObject.Message.cs:1900-1902`） | 同上 | 同上 |
| `@repairdoornow` | `:207-211`；`RepairDoor :358-381` | 无（金币在城内 `TotalGold`） | 门 `BaseObject==null`/`UnderWar`/`HP>=MaxHP` ⇒ false；否则**严格 `>`** 60s（`GetTickCount()-StruckTick`）才回满；死亡门额外 `Death=false`、`IsOpened=false`；`RefStatus()`；成功才 `TotalGold -= RepairDoorPrice` | `SysMsg`：成功绿「修理成功。」；未生效绿「城门不需要修理!!!」；钱不足红「城内资金不足!!!」；随后 `@main` say。出队序：say(1) → sysmsg(252)（调用序是 sysmsg 在前） | `GotoLable(@main)` | P4 不可达 |
| `@repairwallnow1` | `:212-216`；`RepairWallNow :383-406` | 同上 | `RepairWall(1)` → `LeftWall`；同 60s/UnderWar/满血门；成功 HP=max + `RefStatus` + 扣 `RepairWallPrice` | 成功「修理成功。」；失败**「城门不需要修理!!!」**（墙也用门文案，照抄）；钱不足红；`@main` say | `GotoLable(@main)` | P4 |
| `@repairwallnow2` | `:217-221` | 同上 | `CenterWall` | 同上 | 同上 | P4 |
| `@repairwallnow3` | `:222-226` | 同上 | `RightWall` | 同上 | 同上 | P4 |
| `@hireguardnow*` | `:227-232`；`HireGuard :264-309` | 末尾序号（`StrToInt`） | **不可达**：`Substring` 抛 → catch；方法体（金币 300000、`n10<=4`、未雇佣、`!UnderWar`、`RegenMonsterByName`、扣钱、`Castle=castle`、`GuardDirection=3`）永不执行 | 无 | 无 | 内容无链接 ⇒ 双重不可达 |
| `@hirearchernow*` | `:233-238`；`HireArcher :311-356` | 序号 1..12（部署页） | **不可达**：同上；即使修 Substring，`Archers[12]`/`Archers[-1]` 仍越界 | 无 | 无 | P3 点击可达（标签在链接里），但仍静默 |
| `@@castlename` | `:139-154` | `sMsg` = `\r` 后文本，`Trim()` | 非空：`Castle.sName = sMsg`；`Castle.Save()`（`SaveConfigFile` 写**ctor 时**路径 `Castle/Castle/SabukW.txt` 整文件；`SaveAttackSabukWall` 写 `Castle/<ConfigDir>/AttackSabukWall.txt` = 部署 `Castle/0/`）；`MasterGuild.RefMemberName()`（对在线成员逐人 `RefShowName`）；空：不改不存 | `SM_MENU_OK`（Recog=**NPC** ActorId）尾「城堡名称更改成功...」/「城堡名称更改失败!!!」；成员名牌 `SM_USERNAME(42)`（Recog=成员 ActorId，Param=GetChrColor，尾=EncodeString(名字)）。出队序：menu-ok(69) 先于 username(165) | 无脚本记录（`@@` 标签不在 `Npc_def` 里） | P4 不可达；无长度校验（`CastleNameLen=16` 声明了但全仓无消费） |
| `@@withdrawal` | `:155-176` | `sMsg` 数字（`StrToInt(...,0)`） | **N1：抛 `NotImplementedException` → catch；无状态、无金币、无提示** | 无 | `GotoLable` 无记录 | P4；P3 点击静默 |
| `@@receipts` | `:177-198` | 同上 | **N1 同上** | 无 | 同上 | 同上 |
| `@@sendmsg` | `:134-138`；`SendCustemMsg :408-420` | `sMsg` 文本 | `!SubkMasterSendMsg` ⇒ 红「城主发信息功能还没有开放!!!」；否则 `BoSendMsgFlag` 为真 ⇒ 清标志 + 全服广播 `ChrName + ": " + sMsg`（`MsgType.Castle`，走 `SendBroadCastMsg`，`WorldServer.cs:1738-1775`，每收件人 `SysMsg(..., Red, Castle)`）；**不检查 `IsSendMsg`**（与 Merchant 不同） | `SM_MENU_OK`（Recog=NPC；**空尾**）；广播为每个在线者 `SM_SYSMESSAGE(100)`（Recog=收件人；尾=EncodeString）；出队序 menu-ok(69) 先于 sysmsg(252) | 无 | 双重不可达（标签不在页面；`SETSENDMSGFLAG` 不在城堡脚本） |
| `@@offlinemsg` | `:127-133` | `sMsg` | `IsOffLineMsg` 为真才 `SetOffLineMsg`（`OffLineLeaveWord`）；城堡 NPC 的 `IsOffLineMsg` 恒 false（头被 `boFlag=false` 跳过） | 无 | 无 | 不可达 |
| `@exit` | `:239-242` | 无 | 无（关面板） | `SM_MERCHANTDLGCLOSE(644)`（Recog = 调用里的 `nParam1` = NPC ActorId）；base 已发过一条 ⇒ **两条同帧**（同优先级 143，顺序不定） | 无 | 本部署脚本无该链接；客户端关闭按钮是否直发 `@exit`/`RM_MERCHANTDLGCLOSE` 需抓包确认；注入页面后可达 |
| `@back` | `:243-250` | 无 | 无 | 返回页 `SM_MERCHANTSAY`；base 可能已发一份 ⇒ **两份说词** | `GotoLable(ScriptGoBackLable ?: @main)` | 同上 |
| 非 `@` 数据 | `:253-256` | 任意不以 `@` 开头的串 | 无 | `SM_MENU_OK`（Recog=NPC）尾「你没有权利使用.」 | 无 | P5 可达（构造包） |
| `@main`/`@treatdoor`/`@repaircastle`/`@repairdoor`/`@repairwalls`/`@hirearchers`/`@commandorder`/`@aboutwar`/`@day`/`@listwar` | `Npc_def:2-90` | 无 | 纯说词（`@openmaindoor/@closemaindoor` 例外，有 `#ACT`） | `SM_MERCHANTSAY(643)`，尾 = `ChrName + '/' + 展开后的整页` | `GotoLable(label)` | P2/P3 可达 |

### 3.2 每类副作用的口径

- **门/墙 HP**：`Repair*` 只在成功时写 `WAbil.HP = MaxHP`；`CastleDoor.Open/Close` 本身不改 HP；
  `RefStatus` 把 `Dir` 重算并广播。Rust `CastleUnit.hp/max_hp/dir/death/opened` 已能承载。
- **弓手**：`@hirearchernow*` 全不可达（N5）。部署启动时 12 个弓手 HP=0 被 `Initialize` 跳过
  （`UserCastle.cs:174-193`），所以**不存在「用标签补弓手」这条链**；不要为了「让标签有作用」而实现它。
- **行会仓库（金库）**：唯一入口 `@@withdrawal`/`@@receipts`，N1 ⇒ 无效果。`TotalGold` 的**真实**写入
  只有商人税 `IncRateGold`（`UserCastle.cs:768-812`，`crates/world/src/castle_tax.rs`，另一片），
  与 CastleOfficial 无关。
- **税**：CastleOfficial 的标签**不触发任何税**；税在 `Merchant.cs:544-553/1250-1259/1385-1393/1637-1645`
  的买卖/修理路径（`castle_tax.rs` 已建模，属 ⑧-9）。
- **文件**：只有 `@@castlename` 触发 `Save()`；`SaveConfigFile` 的路径怪癖（写 ctor 绑定文件而不是
  `ConfigDir`）见 `design-castle-war-area-20261021.md`；`SaveAttackSabukWall` 则用**当前** `ConfigDir`。
- **广播**：只有 `@@sendmsg` 走 `SendBroadCastMsg(..., MsgType.Castle)`；`MsgType.Castle` 在
  `SysMsg` 里落到 `else` 分支的 default（不是 `Cust`）⇒ 用 **红** 色（`RedMsgFColor/BColor`），
  `ShowPreFixMsg=0` 时无 `CastleMsgPreFix`。

### 3.3 变量文本（CastleOfficial.GetVariableText，`:33-91`）

| token | oracle | 值 / 边界 | 部署期望 |
| --- | --- | --- | --- |
| `$CASTLEGOLD` | `:44-47` | `Castle.TotalGold.ToString()` | `0` |
| `$TODAYINCOME` | `:48-51` | `Castle.TodayIncome` | `0` |
| `$CASTLEDOORSTATE` | `:52-69` | `Death ⇒ "destroyed"`；`IsOpened ⇒ "opened"`；否则 `"closed"`；**`MainDoor.BaseObject` 无 null 检查** | 部署 `BaseObject==null` ⇒ **NRE，整页不显示** |
| `$REPAIRDOORGOLD` | `:70-73` | `Config.RepairDoorPrice` | `2000000`（`Server.conf:308`） |
| `$REPAIRWALLGOLD` | `:74-77` | `Config.RepairWallPrice` | `500000`（`:309`） |
| `$GUARDFEE` | `:78-81` | `Config.HireGuardPrice` | `300000`（`:306`） |
| `$ARCHERFEE` | `:82-85` | `Config.HireArcherPrice` | `300000`（`:307`） |
| `$GUARDRULE` | `:86-89` | 常量 `"无效"`（脚本未用） | `"无效"` |

基类（`NormNpc`）同时参与展开、部署页用到的：

| token | oracle | 口径 | 部署期望 |
| --- | --- | --- | --- |
| `$OWNERGUILD` | `NormNpc.cs:749-765` | `Castle.OwnGuild`；空 ⇒ `"游戏管理"`；Castle null ⇒ `"????"` | `"游戏管理"` |
| `$CASTLENAME` | `:766-778` | `Castle.sName`；null ⇒ `"????"` | `"沙巴克"` |
| `$LORD` | `:779-804` | `MasterGuild.GetChiefName()`；`MasterGuild==null` ⇒ `"管理员"` | `"管理员"`（R-0） |
| `$CASTLEWARDATE` | `:805-831` | `AttackWarList` 空 ⇒ **整行替换**为 `"暂时没有行会攻城 .\\ \\<返回/@main>"`；`UnderWar` ⇒ `"正在攻城当中.\\ \\<返回/@main>"` | 前者 |
| `$LISTOFWAR` | `:832-877` | 空 ⇒ 同上整行替换 | 同上 |
| `$CASTLEGETDAYS` | `:878-` | `HUtil32.GetDayCount(DateTime.Now, Castle.ChangeDate)`；**墙钟** | 相对 `2026-10-09` 的天数（窗口需 mask） |
| `$DATETIME` | `:356-359` | `DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss")`；**墙钟** | 窗口需 mask |

变量层怪行为（判据必须钉）：

1. **大小写敏感**：`switch (sVariable)` 是 C# 字符串 switch（ordinal）⇒ `<$castlegold>` **不展开**；
   而标签比较是 `OrdinalIgnoreCase`。两者口径不同，不许统一成同一套。
2. `ReplaceVariableText`（`NormNpc.cs:1618-1633`）只替换**首次**出现，且是 `IndexOf(..., OrdinalIgnoreCase)`
   —— 名字大小写不敏感地找，但 `sStr.Length` 按原 token 长度切；`GetLineVariableText` 循环最多 101 次。
3. **Castle==null ⇒ `sMsg = "????"` 覆盖整行**（`CastleOfficial.cs:36-40`），不是只替换该 token；
   base 已经替换过的其它变量也一起丢失。
4. `$CASTLEDOORSTATE` 的 NRE 不是「异常安全」：它中止整页说词（`Merchant.UserSelect` 的 catch
   只记日志；P2 直发时异常发生在 base 调用内，同样整页不显示）。
5. 基类 `$CASTLEWARDATE/$LISTOFWAR` 的「整行替换」也是覆盖 `sMsg` 并 `return`。

---

## 4. Rust 现状对照与缺口

### 4.1 已有（可直接复用）

| Rust | 位置 | 覆盖的 oracle |
| --- | --- | --- |
| `door_state_text` | `crates/world/src/castle.rs:463` | `$CASTLEDOORSTATE` 三分支（但**要求调用方确认门存在**，与 NRE 语义的衔接见 §5） |
| `castle_official_action` | `:510` | `@openmaindoor`/`@closemaindoor`/`@repairdoornow`/`@repairwallnow1..3`/`@hire*now*`（N5） |
| `compare_lstr` | `:486` | `HUtil32.CompareLStr`（含多字节不 panic 的 O-1 修正） |
| `hire_substring` | `:538` | N5 `Substring` 恒越界 |
| `official_repair_door/wall` | `:553/:589` | 价格门 + 成功扣款 + 文案 |
| `UserCastle::main_door_control` | `:1398` | 幂等开关 + ghost/null 跳过 + `StructureEvent` |
| `UserCastle::repair_door/wall` | `:1424/:1448` | 60s 严格 `>`、`UnderWar`、满血、死亡清除 |
| `CastleUnit::door_open/close` + `door_map_xy_flag_ops` | `:250-330/:405-455` | `RM_DIGUP/DIGDOWN` 事件 + 15/12 条 `SetMapXyFlag` |
| N1 no-op | `castle.rs:1613/1638`；`castle_tax.rs:502` 测 | `@@withdrawal`/`@@receipts` 可观测 no-op（T-38） |
| 价格配置 | `crates/data/src/guild_castle.rs:106-113/172-175/254-257` | `[Setup] HireGuard/HireArcher/RepairDoor/RepairWall` |
| 部署脚本解析 | `crates/script`（`ScriptParsers`） | `Npc_def` 文本可解析（但固定 NPC 未装载） |
| 说词标签提取 | `bins/game-svr/src/shop.rs:1459 get_script_label`、`:1448 lable_is_can_jmp` | `GetScriptLabel`/`LableIsCanJmp` 的纯函数 |

### 4.2 缺口（按宿主层）

| # | 缺口 | 证据 | 影响 |
| --- | --- | --- | --- |
| G1 | `FindNpc` 未接线：`CM_CLICKNPC` 与 `CM_MERCHANTDLGSELECT` 都只认 `e.merchant.is_some()` | `main.rs:3771/4371`；固定 NPC 由 `npcs.rs` 放成 `Entity.npc=Some`，未设 `merchant` | 城堡 NPC 既点不到、也发不进对话；P2 无法触发 |
| G2 | `Npc_def/<ChrName>-<MapName>.txt` 未装载 | `npcs.rs` 只放置；`npc.rs::NpcHost::load` 吃 `MapQuest`/`Market_Def`；`NormNpc.LoadNPCScript` 的 `IsQuest=true` 口径（`NormNpc.cs:1590-1601`）未覆盖 | `@main` 页与全部脚本标签都不存在 |
| G3 | NPC 侧变量展开未建模 | `crates/script/src/engine/mod.rs:131` 的 `line_variable_text` 缺省原样返回；`ScriptNpc` trait 无 `get_variable_text`；`NpcPlayer` 未覆写 | `$CASTLEGOLD` 等原样上线（可见差异） |
| G4 | `CanJmpScriptLableMap` 生产恒空 | `main.rs:12852` 注释自认；只有测试里 `get_script_label` 填 | 所有标签在 `LableIsCanJmp` 处被丢弃 |
| G5 | `ScriptCurrLable/ScriptGoBackLable/ScriptGotoCount` 会话态未建模 | `ScriptNpc::script_goto_count` 有；`ShopSession` 只有 `script_lable`/`can_jmp_labels`；`NormNpc.UserSelect`（`:1634-1659`）的栈维护无对应 | `@back` 无法正确返回 |
| G6 | 无 CastleOfficial 派发 | `shop::user_select` 只有 Merchant 六支；`castle_official_action` 无生产调用点 | 标签→动作未接线 |
| G7 | 无 `$CASTLEDOORSTATE` 的 NRE 等价 | `door_state_text` 要求 `&CastleUnit`；部署 `main_door=None` | 部署态 `@treatdoor` 应「整页不显示」，而不是显示 `closed` |
| G8 | 无 `SabukW.txt` 写侧 | `envir/src/castle.rs:28` 自认「不写」 | `@@castlename` 的持久化不可实现（且该标签本身不可达） |
| G9 | 无帧出队优先级模型 | 宿主按返回顺序直接 `emit`（`main.rs` shop/castle 路径） | 多帧流程（open/repair/castlename）顺序错 |
| G10 | 门帧（`SM_DIGUP/DIGDOWN/ALIVE`）宿主广播未接线 | `main.rs:17480-17484` 只处理 `SetMapXyFlag`，其余 `glog`；T-127 | 开门/修门对观察者不可见 |
| G11 | `SendRefMsg` 的 500ms `VisibleHumanList` 缓存未建模 | `BaseObject.Message.cs:383-450`（`SendRefMsgTick >= 500` 才重扫 12 格） | 门帧/名牌帧的收件人集合与真机可能不同（时序） |
| G12 | `Permission` 未进会话（恒 0） | `main.rs:12834` `permission: 0`、`:979` | 与 oracle 生产恒 0 **等价**；若未来接，必须保持 Click 门不可达或另开 `S-*` |
| G13 | `ChangeDate` 未进 `UserCastle` | `crates/world/src/castle.rs` 只有 `war_date`；`crates/envir/src/castle.rs:157` 解析 `change_date` 但未适配进世界态 | `$CASTLEGETDAYS` 无法展开（`$CASTLEWARDATE` 用的是 `attack_war_list`，不受影响） |

---

## 5. 设计建议（实现形态）

### 5.1 纯层 API（`crates/world`，L）

新增/扩展（建议放 `castle.rs` 或新 `castle_official.rs`；**不碰 `guild.rs`**）：

```rust
/// `CastleOfficial.GetVariableText` 的纯展开（单 token）。
/// 返回 Err(DoorStateWithoutEntity) 复刻 oracle 的 NRE 中止；None = 本层不认的 token（原样保留）。
pub enum OfficialVarError { CastleNull/*整行 "????"*/, DoorStateWithoutEntity }
pub fn official_variable_text(
    token: &str, castle: Option<&UserCastle>, cfg: &GameSvrConfView,
) -> Option<Result<String, OfficialVarError>>;

/// 标签 → 动作（已有），扩展 castlename/vault/sendmsg/offlinemsg/exit/back 的分类。
pub enum CastleOfficialAction {
    OpenMainDoor, CloseMainDoor, RepairDoor, RepairWall(u8),
    HireUnreachable,           // N5
    VaultUnreachable,          // N1
    Rename,                    // @@castlename
    SendMsg,                   // @@sendmsg
    OfflineMsg,                // @@offlinemsg
    CloseDialog,               // @exit
    Back,                      // @back
}

/// 一次派发的结果：帧（带优先级）+ 世界操作 + 脚本跳转 + 日志。
pub struct OfficialSelectOutcome {
    pub frames: Vec<QueuedFrame>,       // 带 (priority: u8) 供宿主按 oracle 出队序发送
    pub world_ops: Vec<OfficialWorldOp>,// MainDoorControl / Repair* / Rename(save) / RefMemberName
    pub goto: Option<String>,           // @main / ScriptGoBackLable
    pub log: Option<&'static str>,      // "[Exception] TCastleManager::UserSelect... "
}
```

约束：
- 门动作调既有 `main_door_control`/`repair_*`，**不重写**状态机；`StructureEvent` 转 `QueuedFrame`。
- 修门/修墙的 `now_tick` 由宿主注入（`world.clock_ms`），60s 门严格 `>`。
- `door_state_text` 的调用点先判 `main_door.is_none()` → `DoorStateWithoutEntity`（整页中止）。
- N1/N5 保持 no-op：返回空 `frames`/`world_ops`，日志仅 oracle 的常量前缀（N5）或无日志（N1 在
  `CastleOfficial` 的 catch 里落常量前缀；base 的 catch 不会触发，因为接口调用在 Castle 的 try 内）。

### 5.2 宿主层（`bins/game-svr`，与 guildofficial 串行）

1. `npcs.rs`：给固定 NPC 实体同时挂一个「可对话 NPC」句柄（或在 `World` 侧提供 `find_npc(actor_id)`），
   让 `CM_CLICKNPC`/`CM_MERCHANTDLGSELECT` 的 `FindNpc` 分支可达。
2. `npc.rs`：装载 `Npc_def/<ChrName>-<MapName>.txt`（`boFlag=false`），复用 `NpcHost` 的引擎；
   `ScriptNpc` 增加 `get_variable_text(token) -> Option<Result<String, OfficialVarError>>`，
   在 `send_say` 前按 `NormNpc.GetLineVariableText` 的循环（101 次上限、`'>'` 位置门、大小写敏感）展开。
3. 说词后调用 `shop::get_script_label` 填 `ShopSession.can_jmp_labels`（每次 `Clear`），
   并维护 `ScriptCurrLable/ScriptGoBackLable/ScriptGotoCount`。
4. `CM_MERCHANTDLGSELECT`：按 NPC 类型分派：`Merchant` → `shop::user_select`；
   `GuildOfficial` → guildofficial 的实现；`CastleOfficial` → **先调 Merchant 的 base 等价物**
   （脚本 `GotoLable`/`@exit`/`@back` 等），再调 `castle_official_select`（仅在
   `is_master_guild && is_guild_master` 时）。**顺序不可反**。
5. 帧发送：用 `(priority, seq)` 小顶堆（或按优先级稳定排序）复刻 `PriorityQueue<SendMessage, byte>`；
   同优先级保持入队序（oracle 不保证，但这是最不坏的确定性口径；`@exit` 双帧按多重集断言）。
6. 门帧：把 `StructureEvent::RefDigUp/RefDigDown/RefAlive` 按 `SendRefMsg` 的 12 格 + 500ms 缓存广播
   （或先接 T-127 的帧排水；没有它则只能做构造级）。

### 5.3 持久化（C 卡，R）

- 复用/补齐 `crates/envir/src/castle.rs` 的 `serialize_sabukw`（当前只有 `List.txt`/`AttackSabukWall.txt`）；
  必须复刻 `ConfigFile.Save` 的整文件格式（CRLF、键序/段落序、缺键写回）。
- 路径口径：`SaveConfigFile` 用 **ctor 绑定路径**（`LogService`/`SabukW`），`SaveAttackSabukWall` 用
  **当前 `ConfigDir`**。两者不同，判据要分开断言。
- 由于 `@@castlename` 不可达（R-0），该卡的持久化更可能由 `CastleMgr.Save()`（10min）卡驱动；
  建议**不要**为本卡单独实现一份 Serializer。

---

## 6. 写集切分（与 guildofficial 及其他车道）

| 写集 | 谁拥有（本波） | 本设计的态度 |
| --- | --- | --- |
| `bins/game-svr/**` | **guildofficial**（并发中，禁止触碰） | B 卡（宿主）必须**等其让出**后再排；本波只设计不改 |
| `crates/world/src/guild.rs` | **guildofficial** | 不碰。`@@castlename` 的 `RefMemberName` 通过**事件**跨边界（见下） |
| `bins/{db-srv,game-gate,login-gate,sel-gate,login-srv}/**` | encodinglane | 不碰 |
| `docs/window-run-sheet*` | runsheet | 不碰；窗口项由该车道登记 |
| `crates/world/src/{castle.rs,castle_tax.rs}` | 本家族（castle 线） | A 卡唯一写集；`CastleEvent` 可加变体，不改既有语义 |
| `crates/envir/src/castle.rs` | 本家族（与 castle-file-load 线共用） | C 卡的 Serializer；需与该线串行 |
| `crates/data/src/guild_castle.rs` | 已合（⑧ 配置） | 只读；价格已就位 |
| `crates/{protocol,storage}` | 冻结 | 不改；SM ident 全在 |

**与 guildofficial 的具体边界**：
- GuildOfficial 的标签（`@@buildguildnow`/`@@guildwar`/`@requestcastlewarnow*`/`$REQUESTCASTLELIST`）
  属 guildofficial；CastleOfficial 的标签（本文件 §3）属本家族。
- 两者共用「固定 NPC `Npc_def` 装载 + 说词 + CanJmp 表 + 脚本栈」这一层：**必须由一条线先落地**，
  另一条只接自己的 `UserSelect` 分支。建议把共用层作为 B 卡的前置片（可命名为
  `openmir2-m3-fixed-npc-dialog`），其所有者与 guildofficial 不同线时，双方都只能只读等待。
- `@@castlename` 需要刷新在线成员名牌：本家族只产出
  `CastleEvent::GuildRefreshShowName { guild: GuildId }`（或等价事件）；由 `guild_runtime`/宿主消费。
  **不**在 castle 侧直接改 `GuildManager` 的成员表，避免与 `guild.rs` 争写集。
- `CastleMgr.Save()` 的 10min 落盘属 castle 线；`@@castlename` 的即时 `Save()` 复用同一 Serializer。

---

## 7. 判据清单（带牙草案）

> 牙的定义：**改坏生产/纯函数路径 ⇒ 指定具名断言必红**；实现时先制造红、再逐字节还原，
> 并用 `sha256sum -c` 钉住还原后的文件。下列每条都给「改坏动作」。文档车道未运行 cargo。

| ID | 名称 | 断言要点 | 改坏必红 | tier | 离线/窗口 |
| --- | --- | --- | --- | --- | --- |
| C1 | `castle_official_click_gate_vectors` | `Castle=None` ⇒ 红字「NPC不属于城堡!!!」且无 say；`MasterGuild=None && permission=0` ⇒ **零帧零日志**；注入 `permission=3`（仅测试）⇒ `@main` say | 去掉 `Permission>=3` 或 `MasterGuild` 判断 ⇒ 帧数断言红 | L | 离线 |
| C2 | `castle_official_userselect_gate_vectors` | 非城主：base 说词有、castle 动作无；城主行会但 `GuildRankNo!=1`：仍无动作；城主+掌门：动作有；非 `@` 数据 ⇒ 尾「你没有权利使用.」 | 把 `IsGuildMaster` 去掉 / 把门移到 base 之前 ⇒ 红 | L | 离线 |
| C3 | `castle_official_label_dispatch_vectors` | 全标签映射（含大小写、`@hireguardnow3`、`@hirearchernow12`、未知标签 `None`） | 删任一分支/改成前缀误匹配 ⇒ 红（已有测试 `castle_official_labels_dispatch_and_n5_hire_is_unreachable`，扩到新分类） | L | 离线 |
| C4 | `castle_official_door_open_close_vectors` | 未开+open ⇒ 15 条掩码（3 true→9 true→3 false）、`Dir=7`、`IsOpened/StoneMode/HoldPlace`；已开+open ⇒ 空；close ⇒ 12 条、`Dir` 公式、`HoldPlace=true`；死亡/ghost ⇒ 空 | 翻 `boFlag`、删尾 3 条、`Dir` 写错 ⇒ 红 | L（帧面抽样） | 离线 |
| C5 | `castle_official_repair_vectors` | 钱不足 ⇒ 无 HP/无扣款 + 红；`UnderWar`/满血 ⇒ 无扣款 + 绿「城门不需要修理!!!」；60s 严格 `>`（`60000` 不修、`60001` 修）；死亡门清 `Death/IsOpened`；墙 1/2/3 映射，0/4 拒绝；墙失败文案也是「城门不需要修理!!!」 | `>`→`>=`、失败也扣款、墙索引交换、墙文案改成「城墙」⇒ 红 | **R**（时序+结构 HP） | 离线构造 + 窗口 W-CO-6 |
| C6 | `castle_official_vault_n1_vectors` | `@@withdrawal/@@receipts`：帧空、`TotalGold/TodayIncome` 不变、无 panic；与真实 `inc_rate_gold` 对照仍变 | 实现真提取/上缴 ⇒ 红（T-38） | L | 离线 |
| C7 | `castle_official_hire_n5_vectors` | `@hirearchernow0..12`/`@hireguardnow*`：无帧、无状态、无 panic；`hire_substring` 恒 `Err` | 返回 `Ok` 或生成弓手 ⇒ 红（T-40） | L | 离线 |
| C8 | `castle_official_variable_text_vectors` | 8 个 CastleOfficial 变量 + 部署页用到的 7 个基类变量；`$castlegold` 不展开；未知 token 原样；`Castle=None` ⇒ 整行 `"????"`；首次出现替换；101 次上限；`main_door=None` ⇒ `DoorStateWithoutEntity` | 改文案、去 `"????"`、把变量名改成大小写不敏感、门 None 返回 `"closed"` ⇒ 红 | L（墙钟项抽样） | 离线（`$DATETIME/$CASTLEGETDAYS` 注入时钟） |
| C9 | `castle_official_say_label_loop_vectors` | `get_script_label` 对部署页提取的标签集精确（`@main` 页 8 个、`@treatdoor` 3 个、`@repairwalls` 4 个、`@hirearchers` 13 个）；未说过的标签 `lable_is_can_jmp=false`；`@main` 恒真 | 删 `Clear`（跨页残留）或改 `/` 提取 ⇒ 红 | L | 离线 |
| C10 | `castle_official_castlename_persist_vectors` | 空名 ⇒ 失败文案、不 Save；非空 ⇒ trim、成功文案、`Save` 事件、成员名牌事件；`SabukW` 写 ctor 路径、`AttackSabukWall` 写 `ConfigDir`；整文件 CRLF 字节 | 写错目录、漏 `Save`、把 `MasterGuild` null 当允许 ⇒ 红 | **R**（持久化） | 离线字节 + 窗口 W-CO-7 |
| C11 | `castle_official_frame_queue_order_vectors` | 复合流程出队序：open（say 1 → menu-ok 69 → digup 216）；repair（say 1 → sysmsg 252）；castlename（menu-ok 69 → username 165）；sendmsg（menu-ok 69 → sysmsg 252）；`@exit` 双 close 按多重集 | 直接按调用序 emit ⇒ 红 | **R**（时序） | 离线 + 窗口 W-CO-1 |
| C12 | `castle_official_deployment_script_vectors` | 读部署 `Npc_def`（只读）或同字节夹具：`@main` 展开文本（mask `$DATETIME`）、`@aboutwar/@listwar` 整行替换、`@treatdoor` 因门 null 中止、价格 2000000/500000/300000；脚本 sha256 钉死 | 改夹具/改文案/把 NRE 变成正常页 ⇒ 红 | L | 离线（部署件存在时） |
| C13 | `castle_official_non_at_data` | 非 `@` ⇒ `SM_MENU_OK` Recog=NPC、尾「你没有权利使用.」；城主门不影响 | 删 else 或加门 ⇒ 红 | L | 离线 |
| C14 | `castle_official_exit_double_frame` | 城主+掌门 + `@exit` 可跳时 ⇒ 两条 `SM_MERCHANTDLGCLOSE`（同 Recog=NPC）；`@back` ⇒ 两份返回页 say | 跳过 base 调用或去重 ⇒ 红 | L | 离线 |

---

## 8. tier 与窗口项

### 8.1 判级

- 本文档：**L**（纯文档 + 只读）。
- 实现整体：**R**。`tier_why`：
  1. `repair` 的 60s `StruckTick` 严格 `>` 是**时序**；
  2. 门帧/名牌帧经 `SendRefMsg` 的 **500ms 可见列表缓存**广播，收件人集合是时序相关；
  3. `@@castlename` 触发**文件持久化**（`SabukW.txt` 整文件 + `AttackSabukWall.txt`）；
  4. 多帧流程的出队序由 `MsgQueue` 优先级决定（时序）。
- 可拆 L 子卡：C1/C2/C3/C6/C7/C8/C9/C12/C13/C14（纯逻辑/帧编码/变量文本）。
- 按仓库政策：R 卡 `merge_result` 后保持 `in-progress`，窗口项未过不得 `closed`。

### 8.2 窗口项（建议登记到 runsheet 车道）

| ID | 内容 | 需要什么 | 断言 |
| --- | --- | --- | --- |
| W-CO-1 | BotSrv 直发 `CM_MERCHANTDLGSELECT(1011)` 给 `沙巴克城堡管理`（同图 0150、距 ≤15），`sData="@main\r"` | 固定 NPC `FindNpc` 在 oracle 侧已可用；BotSrv 已有 `open_dlg` 原语（T-46） | 捕获 `SM_MERCHANTSAY(643)` 尾（mask `$DATETIME`）：OWNERGUILD=游戏管理、LORD=管理员、CASTLEGOLD=0、TODAYINCOME=0；`Recog`=NPC ActorId |
| W-CO-2 | 接 W-CO-1 后发 `@treatdoor\r` | 同上 | 无 `SM_MERCHANTSAY`；服务端日志恰为 `[Exception] TCastleManager::UserSelect... `（常量行，无栈）——部署 0 实体 ⇒ `$CASTLEDOORSTATE` NRE |
| W-CO-3 | 发 `@repairdoor` / `@repairwalls` / `@hirearchers` / `@commandorder` / `@guardrule_*` | 同上 | 页文本逐字节；价格 2000000/500000/300000；静态防守页文案 |
| W-CO-4 | 发 `@openmaindoor` / `@closemaindoor` | 同上 | 只有脚本 `MESSAGEBOX` + `@main` say；**无** `SM_DIGUP/DIGDOWN`（P4 不可达）；文本「城门已经打开。」/「城门已经关闭。」 |
| W-CO-5 | 发 `@hirearchernow1` / `@hirearchernow12` / `@@withdrawal` / `@@receipts` | 同上 | 无任何帧；无金币/HP 变化；日志常量行（hire/withdrawal 都由 Castle 的 catch 落） |
| W-CO-6 | 修门/修墙/开关门的状态动作 | **无法用 stock 部署**：需 oracle 侧注入城主/`Permission`（不允许改 `E:/MirServer` 或 `src`）⇒ 只能构造级 + owner 豁免；如 owner 批准测试专用注入，再补 | 60s 门（60000/60001）、扣款、`SM_DIGUP/DIGDOWN/ALIVE`、`SetMapXyFlag` 逐格 |
| W-CO-7 | `@@castlename` 持久化 | 需 C 卡实现 + 注入城主 + **临时复制的 Envir 树**（绝不写真部署） | `Castle/Castle/SabukW.txt` 整文件 CRLF 逐字节；`Castle/0/AttackSabukWall.txt` 重写 |

> 窗口注意：`$DATETIME` 与 `$CASTLEGETDAYS` 是墙钟；比较必须 mask 或用固定时钟（oracle 侧无时钟注入，
> 只能 mask，登记为口径差）。

---

## 9. 「可离线实现」与「需真机/内容」的分界

### 9.1 可离线实现（不依赖窗口、不依赖真客户端）

1. `CastleOfficialAction` 全标签分类 + 大小写/前缀语义（C3）。
2. Click/UserSelect 的全部门（含 R-0 的不可达性）与「base 先于 castle」的顺序（C1/C2/C14）。
3. 变量文本展开（8 + 7 个 token、大小写敏感、首次替换、101 上限、`"????"`、门 None 中止）（C8）。
4. 门/墙纯状态：`Open/Close` 的 `Dir`、掩码 15/12、`HoldPlace/StoneMode/IsOpened`；`Repair` 的
   60s 严格 `>`、死亡清除、费用（注入 `now_tick`）（C4/C5）。
5. N1/N5 的 no-op 与「改坏必红」（C6/C7）。
6. `get_script_label` / `lable_is_can_jmp` / 脚本页文本（用部署件的只读副本或同字节夹具）（C9/C12）。
7. 帧 `ident/Recog/Param/尾` 的字节构造与出队优先级模型（纯函数）（C11）。
8. `SabukW.txt`/`AttackSabukWall.txt` 的序列化字节与路径选择（注入 base path）（C10）。

### 9.2 需真机 / 内容 / 注入

1. **真机（可达层）**：P2/P3 的 say 帧字节与顺序；`@treatdoor` 的 NRE 日志；`@openmaindoor` 脚本动作
   但门不动；`@hirearchernowN` 静默；`$DATETIME/$CASTLEGETDAYS` 的墙钟值（mask）。
2. **内容**：`Npcs.txt` 的 type=2 行；`Npc_def/沙巴克城堡管理-0150.txt` 的存在与字节；`SabukW.txt`
   的地图/坐标/价格；怪物库中门/墙名的有无（决定实体是否为 0）；`Server.conf` 的价格/颜色/前缀。
3. **注入/裁决**：P4 的城堡状态动作在 stock 部署**不可达**（R-0）——真机要么由 owner 批准测试专用
   注入（改 oracle 运行时状态，不改内容/源码），要么接受「构造级 + 窗口只覆盖可达层」。
4. **AOI/时序**：`SendRefMsg` 的 12 格与 500ms 缓存、`MsgQueue` 同优先级不稳定序——需要真机观察者
   （两个 bot 同屏）才能完全覆盖。

---

## 10. 未决问题（需要 owner/reviewer 裁决）

1. **可达性降级**：audit #6 目前标 P0；按 §1 证据，stock 部署的 P1/P4 不可达，只有 P2/P3 可达。
   是否把本面降为 P1（对话基础设施）/P2（状态动作）？若不降，请指明真机注入方案。
2. **R-0 是否单独立项**：`MasterGuild` 恒 null 是 ⑧ 家族第 11 个 N1 类断点（`GetCastle(IGuild)` +
   启动读序）。建议在 `T-38` 行续登「R-0」，本文件的判据按「不可达」钉死；是否同意？
3. **共用对话层归属**：固定 NPC 的 `Npc_def` 装载 + 说词 + CanJmp 表 + 脚本栈，是 GuildOfficial 与
   CastleOfficial 的共同前置。由 guildofficial 车道一并做、还是单开 `openmir2-m3-fixed-npc-dialog`？
   两条线的 `bins/game-svr` 写集冲突，必须串行。
4. **`@@castlename` 是否实现**：该标签在 stock 部署不可达；`SabukW` Serializer 却由 `CastleMgr.Save`
   卡也需要。建议本卡只登记「复用 C 卡」，不为它单写 Serializer——是否同意？
5. **帧序模型**：宿主当前按返回序直接 emit。是否在本卡就引入 `(priority, seq)` 队列（影响所有 NPC 帧），
   还是先由「帧序/队列」专卡统一改，CastleOfficial 只提供带 priority 的帧描述？
6. **`@treatdoor` 的 NRE**：Rust 用 `Err(DoorStateWithoutEntity)` 中止整页，还是用
   `EngineError::UnsupportedVariable`（现有枚举，`crates/script/src/engine/mod.rs`）？两者可见后果相同
   （无 say、日志一条），但错误分类影响后续诊断；建议新增专用变体。
7. **`@exit`/`@back` 双帧**：是否确认照搬（base + castle 各一次）？如果 Rust 把 base 与 castle 合并成
   一个派发器，必须显式保留两次副作用，否则真机帧数会少一条/一份。
8. **`CastleNameLen=16`**：oracle 声明了但全仓无消费；`@@castlename` 不校验长度。确认照抄（不校验）。
9. **`@@sendmsg` 的 `IsSendMsg` 语义**：CastleOfficial 不检查 `IsSendMsg`，但标签不在页面、`SETSENDMSGFLAG`
   也不在城堡脚本 ⇒ 双重不可达。是否只写判据、不实现广播路径？
10. **部署实体 0 条**：`Castle/Castle/SabukW.txt` 的名字不在怪物库，`@treatdoor` 永远 NRE。是否接受
    「部署态不显示门状态页」作为 parity 目标（而不是为它补内容/补怪物行）？

---

## 11. 收尾与残留物

- 本车道只新增本文件（`mir2-rs/tests/parity/m3/design-castle-official-20261022.md`），一条 docs 提交。
- 未运行 cargo（无 `CARGO_TARGET_DIR`、无构建产物）；未起任何进程/端口；未写数据库；
  未修改 `E:/MirServer/**`（仅只读核对）；主检出 `master` 未改动。
- 残留物清单：无。worktree `/e/tmp/wt-castleofficial` 保留至评审/合并完成后由收尾流程删除。
