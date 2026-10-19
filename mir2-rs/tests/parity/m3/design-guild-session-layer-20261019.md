# Lane 3 — 行会会话层（guild session layer）实施简报

- 基线：`41bd400cd`（master，2026-10-19 工作树快照）。
- 本车道：**只读**（未建 worktree、未改仓库、未起栈、未碰 `E:/MirServer`），只写本文件到 `E:/tmp`。
- 输入：`mir2-rs/tests/parity/m3/audit-uncovered-surfaces-20261019.md` 的 #1/#2/#3 + D-4；
  `mir2-rs/tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md`（下称「⑧盘点」）；
  `crates/world/src/guild.rs`、`crates/data/src/guild.rs`、`bins/game-svr/src/main.rs`、`bins/game-svr/src/smframe.rs`，
  以及已落地的夹具骨架 `M3⑧公会攻城动作序列夹具骨架报告-20261013.md` / `guild_castle_scenario.json` /
  `guild_castle_diff.py` / `run_guild_castle_pair.ps1`。
- 结论定位：**80% 的零件已在仓库里（状态机、文件层、配置层、夹具/比较器），缺的是「最后一公里」接线**：
  9 个 `CM_GUILD*` 派发、全部行会帧编码、会话态 `MyGuild/GuildRankNo/AllowGuild`、登录 750、
  掉线 `DelHumanObj`、持久化与 10s/30s 时序、以及建会/宣战所需的 NPC 入口。
- 下文行号均为本仓库 `src/`（C# oracle）与 `mir2-rs/**`（Rust）实测。

---

## 0. 先纠正三条容易写错的口径（以本节为准）

1. **`GuildManager.Run` 不是每 tick，也不是 30s 节流**：oracle `SystemProcessor.ProcessGameRun`
   （`src/GameSrv/Word/Threads/SystemProcessor.cs:71-77`）用 **10s 门**（`> 10000`）调
   `GuildMgr.Run()`；`Run` 内每会调 `CheckSaveGuildFile`（`GuildManager.cs:145-167`），
   后者才是 **30s** 门（`GuildInfo.cs:525-533`，`BoChanged && (now - DwSaveTick) > 30000`）。
2. **`UpdateGuildFile` 是「立即落盘 + 标记」**：`GuildInfo.cs:584-589` = `BoChanged=true;
   DwSaveTick=now; SaveGuildInfoFile();`。Rust `crates/world/src/guild.rs:517-524` 目前只落标记/不落 IO
   （⑧-3 的口径），**接线时每个调用点都要真的写盘**，不能只等 30s 检查。
3. **登录 750 不是「无会也发空尾」**：`SM_CHANGEGUILDNAME(750)` 只由 `RM_CHANGEGUILDNAME` 触发
   （`PlayObject.Message.cs:1931-1933`），而该 RM 只在 `GuildInfo.GetRankName`（`GuildInfo.cs:485-509`，
   上线物化时）与 `PlayObject.RefRankInfo`（`PlayObject.Base.cs:2655-2660`）里 `SendMsg`。
   ⇒ **无行会登录 = 一条 750 都不发**；「空尾 750」只出现在「MyGuild 已被清空后 `RefRankInfo(0,"")`」
   （删成员/解散路径）。audit #1 写的「空尾或名/封号」需按此修正。

另两条可达性事实（影响判据取舍）：

- **`AllowGuild` 默认 false**（`src/M2Server/Player/PlayObject.cs:903/1007` 区段，ctor 置 `false`），
  唯一写点是 GM 命令 `@Letguild`（`src/Modules/GameCommand/Commands/LetGuildCommand.cs:12`，
  `AllowGuild = !AllowGuild`）。本仓 `bins/game-svr/src/chat.rs` 尚未接 `@` 命令
  ⇒ **生产可达的加成员结果恒为失败码 5**（除非以后接 GM 命令/脚本）。实现仍必须完整建模该字段，
  但判据里要有一个 `AllowGuild=false ⇒ 758 Recog=5` 的具名断言。
- **`EnableAuthAlly` 永远 false**：只有 ctor 置 `false`（`GuildInfo.cs:118`）；唯一的取反
  `AuthallyCommand.cs:13` **整行被注释** ⇒ `CM_GUILDALLY` 在与人对面且双方掌门时仍恒返回
  `769 Recog=-4`（不是 -2/-3 的成功路径）。Rust `GuildState` 甚至没有该字段（可接受：常量 false），
  但判据必须锁死「-4，且 768 永不出现」。

---

## 1. 9 个 `CM_GUILD*` 逐条对照表

### 1.1 公共收包口径（先钉死，所有行共用）

- 线缆 ident：`CM_OPENGUILDDLG=1035` / `GUILDHOME=1036` / `GUILDMEMBERLIST=1037` /
  `GUILDADDMEMBER=1038` / `GUILDDELMEMBER=1039` / `GUILDUPDATENOTICE=1040` /
  `GUILDUPDATERANKINFO=1041` / `GUILDALLY=1044` / `GUILDBREAKALLY=1045`
  （C# `src/OpenMir2/Messages.cs:93-103`；Rust `crates/protocol/src/messages.rs:82-92`，两侧一致）。
- `CM_1042(1042)` 在两次之间，oracle 只记一条「非法数据」日志（`PlayObject.Message.cs:995-997`）
  ⇒ Rust 不得把它误并进 GUILD 匹配臂。
- 转发映射（`src/GameSrv/Word/WorldServer.cs:824-835`，与 default 支 `:851-853` 同形）：
  `playObject.SendMsg(playObject, ident, wParam=defMsg.Series, nParam1=defMsg.Recog,
  nParam2=defMsg.Param, nParam3=defMsg.Tag, sMsg=buff)`。
  ⇒ **9 条都只消费尾串 `Msg`（或完全不消费）；`Recog/Param/Tag/Series` 对业务无意义**。
- `Msg` 的字节口径：客户端 EDCode → 网关 EDCode 解码 → 服端 GBK 解码为 `string`
  （Rust 侧已是 `GateEvent::Command.tail: Option<String>`，`bins/game-svr/src/main.rs:15022-15026`）。
- oracle 执行位置：消息泵 `PlayObject.Message.cs:975-1007`（`Operate` 分支）；
  Rust 侧现有同类入口是 `sim_loop` 的 `else if` 链
  （`bins/game-svr/src/main.rs:12937` 起；组队派发在 `:13191`，交易在 `:12989`，寄售在 `:13015`）。

### 1.2 对照表

| # | CM | 请求字段 | oracle 处理函数（文件:行） | 回包 ident 与字段布局 | Rust 可复用（`crates/world/src/guild.rs`） | 缺口 |
|---|---|---|---|---|---|---|
| 1 | `CM_OPENGUILDDLG` 1035 | 无 | `PlayObject.Operate.cs:1229-1277`（`ClientOpenGuildDlg`） | 有会：`753`，`Recog=0/Param=0/Tag=0/Series=1`，尾 `EncodeString(面板文本)`；无会：`754` 纯头 | `GuildManager::member_of_guild` + `GuildState`（`notices/wars/allies/ranks`） | **面板文本构造器不存在**（需新增，见 §2.2）；派发/会话态缺失 |
| 2 | `CM_GUILDHOME` 1036 | 无 | `Operate.cs:1278-1281`（`ClientGuildHome` → 同 1） | 同 1 | 同 1 | 同 1 |
| 3 | `CM_GUILDMEMBERLIST` 1037 | 无 | `Operate.cs:1283-1306` | 有会：`756`，`Recog=0/Param=0/Tag=0/Series=1`，尾 `EncodeString(text)`；无会：**不发任何帧**（早退） | `GuildState::member_list_text`（`:633-655`，含 `>5000` UTF-16 门） | 帧编码器缺失；派发/会话态缺失 |
| 4 | `CM_GUILDADDMEMBER` 1038 | 尾 = 目标角色名 | `Operate.cs:1307-1368` | 成功 `757` 纯头；失败 `758`，`Recog=nC∈{1..5}` | `GuildState::is_member` / `ranks.len() < GUILD_RANK_LIST_LIMIT(400)` / `add_member` / `get_rank_name`；`GuildManager::find_guild_mut` | `AllowGuild` 字段不存在；在线目标 + `GetPoseCreate()==this` 需宿主（`main.rs:2181` 的 `pose_create` 可复用）；`GUILD_RANK_LIST_LIMIT` 定义了但**无使用点**；目标侧 750/42/系统提示的帧序未接 |
| 5 | `CM_GUILDDELMEMBER` 1039 | 尾 = 目标角色名 | `Operate.cs:1369-1425` | 成功 `759` 纯头；失败 `760`，`Recog=nC∈{1..4}` | `is_member` / `del_member` / `cancel_guld` / `GuildManager::del_guild` | 会话 `GuildRankNo` 缺失；被删者 `MyGuild=null + RefRankInfo(0,"")` 需落成 750 纯头；`SS_206/207` 事件未建模；掌门自删=解散的侧效应未接 |
| 6 | `CM_GUILDUPDATENOTICE` 1040 | 尾 = 以 `\r` 分段的公告 | `Operate.cs:1426-1443` | 成功：立即落盘 + `SS_207` + 重开面板（`753`）；守卫不过：**静默无帧** | `GuildState.notices`；`get_valid_str3`（`crates/data/src/guild.rs:404` 或 `mir2_shared::hutil32`）；`update_guild_file` 仅标记 | 缺「`\r` 分段填 `NoticeList`」小函数；缺 `SaveGuildInfoFile` 的立即落盘调用；缺 `ClientOpenGuildDlg` 复用 |
| 7 | `CM_GUILDUPDATERANKINFO` 1041 | 尾 = `#rankNo rankName\r成员…` | `Operate.cs:1444-1463`；`GuildInfo.cs:708-993` | `nC==0`：`SS_207` + `756`；`nC<=-2`：`761` `Recog=nC`；`nC==-1` 或守卫不过：静默 | `GuildState::update_rank`（`:657-841`，含 **N3 成功清空 `ranks`**） | 761 编码器；SS_207；成功路径的立即落盘；成员列表复用 |
| 8 | `CM_GUILDALLY` 1044 | 无（用 `GetPoseCreate`） | `Operate.cs:1465-1523` | 成功 `768` 纯头；失败 `769`，`Recog=n8∈{-1,-2,-3,-4}` | `is_ally_guild` / `is_not_war_guild` / `ally_guild`（可观测 no-op）/ `ref_member_name` | `EnableAuthAlly` 字段（恒 false，显式常量即可）；两条 `SendGuildMsg`（`SM_GUILDMESSAGE 104`）未接；`SS_207`×2；双方向 `RefMemberName`→42 帧 |
| 9 | `CM_GUILDBREAKALLY` 1045 | 尾 = 行会名 | `Operate.cs:1525-1560` | 成功 `770` 纯头；失败 **`769`（不是 771）且 `Recog=0`**（oracle 原样）；非掌门：静默 | `is_ally_guild` / `del_ally_guild` / `ref_member_name` | `FindGuild` 是大小写敏感 `==`（`GuildManager::find_guild`）；联盟表恒空 ⇒ 生产恒 `769/0`；`771` 永不发 |

### 1.3 各处理函数内的检查顺序（避免「看起来等价」）

- **AddMember**（`Operate.cs:1307-1368`）严格顺序：
  `IsGuildMaster()` → `GetPlayObject(name)!=null` → `playObject.GetPoseCreate()==this` →
  `playObject.AllowGuild` → `!MyGuild.IsMember(name)` →
  `playObject.MyGuild==null && MyGuild.RankList.Count<400`（**是 rank 数不是成员数**）→
  成功副作用：`AddMember`（建/找 rank 99，`UpdateGuildFile`）→ `SS_207` →
  `playObject.MyGuild=MyGuild` → `GetRankName`（⇒ 目标 750）→ `playObject.GuildRankNo` →
  `RefShowName`（⇒ 周围 42）→ 目标 `SysMsg("你已加入行会: {会名} 当前封号为: {封号}", Green, Hint)`。
  失败码：1=非掌门；2=目标不在线或没面对；3=已在**本会**；4=已在别的会 **或** rank 数 ≥400；5=`AllowGuild=false`。
- **DelMember**（`:1369-1425`）：1=非掌门；2=不是本会成员；3=自删（但 `CancelGuld` 成功时最终置 0）；4=`DelMember` 失败；
  成功：被删者 `MyGuild=null; RefRankInfo(0,"")`（⇒ 750 纯头）→ `SS_207`；自删成功：`GuildMgr.DelGuild` →
  `SS_206` → 自己 `MyGuild=null; RefRankInfo(0,"")` → `SysMsg("行会{名}已被取消!!!")`。
- **UpdateNotice**（`:1426-1443`）：`MyGuild==null || GuildRankNo!=1` ⇒ return；
  `NoticeList.Clear()`；`while !IsNullOrEmpty` `GetValidStr3(sNotict, ref sNoticeStr, '\r')` 逐段 `Add`；
  `SaveGuildInfoFile()`（**不经过 BoChanged**）→ `SS_207` → 重开面板。
- **UpdateRank**（`:1444-1463`）：守卫同上；`nC==0` ⇒ `SS_207` + `ClientGuildMemberList`；`nC==-1` ⇒ 静默；
  `nC<=-2` ⇒ `761 Recog=nC`。
- **Ally**（`:1465-1523`）：`n8=-1` 起始；`GetPoseCreate()!=null && Race==Play && posePlayer.MyGuild!=null &&
  posePlayer.GetPoseCreate()==this` 才继续；`!posePlayer.MyGuild.EnableAuthAlly` ⇒ -4；
  `!(posePlayer.IsGuildMaster() && IsGuildMaster())` ⇒ -3；
  `!(MyGuild.IsNotWarGuild(other) && other.IsNotWarGuild(MyGuild))` ⇒ -2；成功 0。
  **当前部署永远停在 -4**。
- **BreakAlly**（`:1525-1560`）：`!IsGuildMaster()` ⇒ return；`FindGuild(name)`（大小写敏感）；`MyGuild.IsAllyGuild(guild)`
  ⇒ 成功；否则 `769 Recog=0`。

---

## 2. 帧清单（需要新增/接线的全部行会帧）

`smframe.rs` 现状：`grep -n GUILD` **无命中**（73 个 `sm_*` 均为非行会帧）。以下编码函数**全部要新增**；
公共帮助已备：`command_header`（`smframe.rs:57-65`）、`gm_data_body`（`:69-74`）、`edcode::encode`、
`downlink::gbk` / `enter::gbk`。所有 `SM_*` 走 `SendDefMessage` 的 12B 头布局
（`PlayObject.Socket.cs:17-45`：`Recog(i32)/Ident(u16)/Param(u16)/Tag(u16)/Series(u16)`）。

| ident | 常量 | 发点（oracle） | 头字段 | 尾 | smframe | 备注 |
|---|---|---|---|---|---|---|
| 750 | `SM_CHANGEGUILDNAME` | `PlayObject.Base.cs:1625-1637`（`SendChangeGuildName`），由 `GuildInfo.cs:499` / `PlayObject.Base.cs:2659` 的 `RM_CHANGEGUILDNAME` 触发 | `0,0,0,0` | 有会：`EncodeString("{GuildName}/{GuildRankName}")`；无会：**纯头** | 缺 | 登录（`PlayObject.cs:1391-1400`）与每次 `GetRankName`/`RefRankInfo` |
| 753 | `SM_OPENGUILDDLG` | `Operate.cs:1266-1267` | `Recog=0,Param=0,Tag=0,Series=1` | `EncodeString(panel)` | 缺 | 面板文本见 §2.2 |
| 754 | `SM_OPENGUILDDLG_FAIL` | `Operate.cs:1274` | 全 0 | 无 | 缺 | 无会 |
| 756 | `SM_SENDGUILDMEMBERLIST` | `Operate.cs:1303-1304` | `0,0,0,Series=1` | `EncodeString(member_list_text)` | 缺 | 文本已由 `GuildState::member_list_text` 产出 |
| 757 | `SM_GUILDADDMEMBER_OK` | `Operate.cs:1361` | 全 0 | 无 | 缺 | |
| 758 | `SM_GUILDADDMEMBER_FAIL` | `Operate.cs:1365` | `Recog=nC` | 无 | 缺 | nC 是 byte 语义的 i32 |
| 759 | `SM_GUILDDELMEMBER_OK` | `Operate.cs:1411` | 全 0 | 无 | 缺 | |
| 760 | `SM_GUILDDELMEMBER_FAIL` | `Operate.cs:1415` | `Recog=nC` | 无 | 缺 | |
| 761 | `SM_GUILDRANKUPDATE_FAIL` | `Operate.cs:1458` | `Recog=nC`（负数） | 无 | 缺 | `nC <= -2` 才发 |
| 762 | `SM_BUILDGUILD_OK` | `PlayObject.Message.cs:1938-1939`（`RM_BUILDGUILD_OK`） | 全 0 | 无 | 缺 | 由 NPC `GuildOfficial.cs:172` 触发 |
| 763 | `SM_BUILDGUILD_FAIL` | `PlayObject.Message.cs:1940-1942` | `Recog=processMsg.nParam1`（= result，负数） | 无 | 缺 | `RM_BUILDGUILD_FAIL(10304)` 的内部参数 |
| 768 | `SM_GUILDMAKEALLY_OK` | `Operate.cs:1509` | 全 0 | 无 | 缺 | 当前不可达（-4 恒失败） |
| 769 | `SM_GUILDMAKEALLY_FAIL` | `Operate.cs:1513` **和 `:1555`** | `Recog=n8`（0..-4） | 无 | 缺 | **BreakAlly 失败也发 769/0** |
| 770 | `SM_GUILDBREAKALLY_OK` | `Operate.cs:1551` | 全 0 | 无 | 缺 | 当前不可达 |
| 771 | `SM_GUILDBREAKALLY_FAIL` | **无发点** | — | — | 缺 | 常量存在但 oracle 不用；**不要实现成默认失败帧** |
| 104 | `SM_GUILDMESSAGE` | `PlayObject.Message.cs:1699-1721`（`RM_GUILDMESSAGE`） | `Recog=processMsg.ActorId`（**收件人自己**）、`Param=MakeWord(FColor,BColor)`、`Series=1` | `EncodeString(prefix+text)` | 缺 | 仅在 `BanGuildChat==true`（默认 true）时发；`ShowPreFixMsg=0`（现网）⇒ 无前缀 |
| — | `RM_BUILDGUILD_OK/FAIL` 10303/10304 | `GuildOfficial.cs:172/176` 内部 `SendMsg` | — | 不直接上线 | — | 由 `PlayObject.Message.cs:1938-1942` 转成 762/763 |
| 767 | `SM_MENU_OK` | `PlayObject.Message.cs:1952-1953` | `Recog=nParam1` | `EncodeString(Msg)` | 有（`sm_menu_ok`，`smframe.rs:392`） | **不属于 9 CM 行会链**；来自 `CM_CLICKNPC` 的 `!IsCanDeal`（`PlayObject.Base.cs:676-680`）等。夹具场景里的 767 属 NPC 链，别混入行会派发判据 |

### 2.1 `RM_CHANGEGUILDNAME` → 750 的队列语义（登录判据的关键）

- `GetRankName` 用 `playObject.SendMsg(playObject, RM_CHANGEGUILDNAME, ...)`（`GuildInfo.cs:499`），
  `BaseObject.Message.cs:34-96` 把优先级取 **`(byte)wIdent` = 10301 & 0xFF = 61**。
- 登录序列（`PlayObject.cs:1385-1400`）：六条 `SendPriorityMsg`（0/1）之后才 `MemberOfGuild` → `GetRankName`，
  ⇒ 在现有 `EnterWorldInputs::frames()` 的模型 C′ 里，750 应**在 `46` 之后、`654`（优先级 153）之前**出队。
  模型与实现位置：`bins/game-svr/src/smframe.rs:1320-1490`（`LoginQueued` + 4 叉堆）。
- **tier=R**：这是「出队序」，必须在真机窗口用同一会话对拍（现有多会话观测只覆盖了 52/752 序）。
  不要把「按优先级插进去」当成已证明。

### 2.2 面板文本（753/756）的精确构造（建议放在 `crates/world`，纯函数）

`ClientOpenGuildDlg`（`Operate.cs:1229-1277`）拼接顺序（`sSendStr`）：

```
{GuildName}\r \r                      ← 注意第二行是一个空格
'1'\r  若 GuildRankNo==1，否则 '0'\r
"<Notice>"\r
 逐条 NoticeList[i] + '\r'           ← 每轮前判 sSendStr.Length > 5000（UTF-16 code unit）
"<KillGuilds>"\r
 逐条 GuildWarList[i].ToString() + '\r'  ← 每轮前判 >5000
"<AllyGuilds>"\r
 逐条 GuildAllList[i] + '\r'         ← 每轮前判 >5000（当前恒空）
```

- `GuildWarList[i]` 是 **record struct**（`src/Modules/SystemModule/Data/WarGuild.cs:5-10`）：
  `ToString()` 输出 `WarGuild { Guild = {IGuild.ToString()}, WarTick = {n}, WarTime = {n} }`；
  `GuildInfo` 未重写 `ToString` ⇒ 非空时打印 `M2Server.Guild.GuildInfo`，空时 `Guild = ` 后为空。
  这是 **oracle 现状**（⑧盘点 §1.B），必须逐字节照搬，不许「美化」成行会名。
- 建议函数：`GuildState::open_dlg_text(&self, guild_rank_no: i16) -> String`（`crates/world/src/guild.rs`），
  返回未编码的 UTF-16 语义文本；`smframe` 只做 `edcode::encode(gbk(text))`。
- `member_list_text` 已存在且逐字节对（`guild.rs:633-655`）。

---

## 3. 文件持久化（GuildStore / GuildManager）

### 3.1 现有实现与调用点缺口

| 环节 | oracle | Rust 现有 | 状态 |
|---|---|---|---|
| 名单加载 | `GuildManager.cs:78-110` | `GuildStore::load_guild_info`（`crates/data/src/guild.rs:333-402`）+ `GuildManager::load_guild_info`（`crates/world/src/guild.rs:928-932`） | ✅ 实现，**零生产调用点** |
| 单会加载 | `GuildInfo.cs:195-346` | `GuildStore::load_guild` / `parse_guild_file`（`data/guild.rs:276-301/485-595`） | ✅，含 `s20` 残留、联盟丢弃两处 oracle 怪行为 |
| 名单落盘 | `GuildManager.cs:124-143` | `GuildStore::save_guild_list`（`data/guild.rs:246-260`） | ✅（非 0 区 no-op；目标不存在 ⇒ `MissingSaveTarget`） |
| 行会文件落盘 | `GuildInfo.cs:366-431` | `GuildStore::save_guild` + `serialize_guild_file`（`data/guild.rs:303-331/597-646`） | ✅（GBK + CRLF；`+`/`#`/四段/空格分隔行） |
| `.ini` | `GuildInfo.cs:195-199` + `GuildConf.cs:8-33` | `GuildStore::load_guild_conf`（`data/guild.rs:262-274`） | ✅ 读侧 + 建空文件；**无写盘**（oracle `SaveGuildConfig` 只改内存，从不 `Save()`） |
| 30s 检查 | `GuildInfo.cs:525-533` | `GuildState::check_save_guild_file`（`world/guild.rs:508-515`） | ⚠️ 只返回 bool，**`GuildManager::run` 丢弃返回值**（`:954`） |
| `Run` | `GuildManager.cs:145-167` | `GuildManager::run`（`world/guild.rs:933-968`） | ⚠️ 逻辑在，**无调度、无落盘** |
| `.bak` | `GuildInfo.cs:591-621` | `GuildState::backup_guild_file`（`:526-537`） | 只清状态；oracle 的 `.bak` 写因目标文件不存在被 `SaveGuildFile` 的 catch 吞掉 ⇒ **永远不会有 .bak 文件**；Rust 只需「尝试失败 + 清空 + 落空文件」 |

### 3.2 启动加载（必须最先做）

1. `GuildCastleConfig::from_ini(&IniFile::from_bytes(Server.conf))`（`crates/data/src/guild_castle.rs:197-...`）
   ——现 `bins/game-svr/src/config.rs:290-...` 是手写解析，**没有** guild/castle 字段；
   建议在 `main()` 里用同一份 raw 文本构 `IniFile` 再取 `GuildCastleConfig`，塞进 `EnterConfig`
   （`main.rs:11605` 结构）。
2. `base = mapwire::base_path(&conf)`（`main.rs:408` 附近）⇒ `GuildStore::new(base.join(guild_dir),
   base.join(guild_file))`。
3. 在 `sim_loop` 里、**时钟注入（`world.clock_ms = origin`）之后**调
   `store.load_guild_info(&sections, rank_name_len, world.clock_ms as i32, server_index)`
   → `GuildManager::load_guild_info(&records)`。
   顺序对齐 `GameApp.InitializeWorld` 的 `GuildMgr.LoadGuildInfo()` 先于 `CastleMgr.LoadCastleList()`
   （`src/GameSrv/GameApp.cs:245-246`）。
4. 失败口径：`MissingListFile` ⇒ oracle 只记 `LogService.Error("行会信息文件未找到!!!")`，会表为空，
   **不拒绝启动**（`GuildManager.cs:106-109`）。

### 3.3 每个变更点的落盘调用（一个都不能漏）

| 动作 | oracle 落盘 | Rust 接线 |
|---|---|---|
| `AddGuild` | `SaveGuildList`（`GuildManager.cs:27`）+ ctor 建 `.ini` + `SetGuildInfo→SaveGuildInfoFile`（`GuildInfo.cs:480`） | `store.load_guild_conf(name)`（建空 `.ini` 副作用）→ `store.save_guild_list(names, server_index)` → `store.save_guild(record, ...)`（新会 `<名>.txt` 不存在 ⇒ 返回 `MissingSaveTarget`，**按 oracle 吞掉**） |
| `DelGuild` | `BackupGuildFile`（`.bak` 失败被吞 + 清空 + `SaveGuildInfoFile`）→ `SaveGuildList` | `save_guild`（清空后的 body）→ `save_guild_list` |
| `AddMember` / `DelMember` | `UpdateGuildFile`（**立即** `SaveGuildInfoFile`） | `store.save_guild` |
| `UpdateNotice` | `SaveGuildInfoFile`（不置 `BoChanged`） | `store.save_guild` |
| `UpdateRank` 成功 | `UpdateGuildFile` | `store.save_guild`（此时 ranks 已被 N3 清空） |
| `AllyGuild` / `DelAllyGuild` | `SaveGuildInfoFile` | `store.save_guild`（联盟表恒空，字节不变） |
| `AddWarGuild` | `UpdateGuildFile` | `store.save_guild`（`n14<=0` 的条目被序列化跳过） |
| `Run` 到期 | `UpdateGuildFile` | `store.save_guild` |
| `CheckSaveGuildFile` | 30s 后 `SaveGuildInfoFile` | `run` 需要把触发者返回给宿主（见下） |
| `DelHumanObj` | 入口先 `CheckSaveGuildFile` | 同上 |

**API 小改**：`GuildManager::run` 现在返回 `Vec<GuildWarEnded>`，而 `check_save_guild_file` 的 `true`
被丢弃（`world/guild.rs:954`）。建议改成返回 `GuildRunOutcome { ended: Vec<GuildWarEnded>,
save: Vec<String> }`（或给 `GuildState` 加一个仅供宿主用的 `pending_save_epoch`），
否则 30s 检查形同虚设；改后同步更新 `world/guild.rs:1423-1442` 的既有单测。

### 3.4 时序接线（tick 的哪一段）

- oracle：`SystemProcessor.ExecuteInternal` 每 50ms 跑 `WorldEngine.Run` 等，最后 `ProcessGameRun`
  在 **10s 门**内调 `GuildMgr.Run()` + `CastleMgr.Run()`（`SystemProcessor.cs:39-77`）。
- Rust：`sim_loop` 每 200ms 一轮；建议在**本 tick 的世界 tick + 全部下行 drain 之后**（即 `main.rs:14345`
  `tick_world_with_specials` 与其后各 `drain_*` 之后、事件循环下一轮之前）加：
  ```
  if world.clock_ms - guild_run_tick > 10_000 {
      guild_run_tick = world.clock_ms;
      let out = guilds.run(world.clock_ms as i32);
      // 逐 ended 发 SM_GUILDMESSAGE(104)；逐 save 调 store.save_guild
  }
  ```
- **时钟域**：`GuildState` 的全部 `now_tick` 与 `CheckSaveGuildFile` 都用 `world.clock_ms as i32`
  （oracle `HUtil32.GetTickCount` 的等价物，见 `world/guild.rs` 模块头）。不要在同一个状态机里混用
  `now_ms()`。
- **一个真实差异（务必登记）**：生产未注入时钟时 `world.clock_ms` 从 0 起，而 C# `Environment.TickCount`
  通常是「开机毫秒数」（远大于 30s）⇒ 加载后 `bo_changed=true, dw_save_tick=0` 的第一次 `Run`
  在 C# 会立即补写、Rust 要等 30s。**行会真机窗口必须设 `MIR2_CLOCK_MS` 为墙钟量级**（与 C# 同起点），
  否则 10s/30s 判据不可判。文件字节通常不受影响（加载出的 war 条目 `WarTime=0` 会被跳过），但时序判据受影响。

### 3.5 掉线

- oracle `PlayObject.Disappear()`：先组队退组，再 `if (MyGuild != null) MyGuild.DelHumanObj(this)`
  （`PlayObject.Message.cs:2138-2147`）。
- Rust 断线路径：`main.rs:13300-13360`（`GateEvent::Closed` 支，在 `online_take`/`save_human_rcd`/
  `ground_sessions.remove` 之间）与 `drain_kicks`（`:11171-11230`）。
  在组队退组之后、会话态移除之前，按会话的 `guild_name` 找会调 `del_human_obj(name, world.clock_ms as i32)`。
- **牙**：`DelHumanObj` 的 oracle 怪行为是 `string.Compare(...)==1`（只命中「大于」，
  `GuildInfo.cs:542`；Rust `del_human_obj` `:494-506` 已照搬），断线名与成员名的排序关系决定是否清空。
  判据要同时覆盖「大于 ⇒ 变空串」和「小于/等于 ⇒ 不动」。

---

## 4. 会话态与角色侧接线（`MyGuild/GuildRankNo/AllowGuild`）

建议在 `SessionGround`（`main.rs:11396`）新增一个内嵌 `GuildSession`（避免把 4 个字段散落）：

```rust
#[derive(Clone, Debug, Default)]
struct GuildSession {
    guild_name: Option<String>, // oracle PlayObject.MyGuild（会话内缓存；manager 是真源）
    rank_no: i16,               // GuildRankNo（0 = 无）
    rank_name: String,          // GuildRankName
    allow_guild: bool,          // AllowGuild，ctor=false，不持久化
    ban_guild_chat: bool,       // BanGuildChat，ctor=true（名字反向，true=可收行会消息）
}
```

- 登录物化：`member_of_guild(chr_name)` → `get_rank_name(chr_name)`（返回 `(rank_no, rank_name)`），
  写入会话；**有会才产 750**（见 §2.1）。
- 建会/加成员成功时同样写会话；删成员/解散时清空。
- 掉线/小退/顶号：清会话 + `del_human_obj`。
- `main.rs:2579-2580` 的 `has_guild:false / guild_rank_no:0` 改为读会话（Need 6/7/60/70，`bag.rs:368-391`）。
- `main.rs:3095-3097` 的 `SM_SENDUSERSTATE` 的 `guild_name/guild_rank_name` 改为读会话
  （`queryuser::build_user_state_buffer` 已支持这两个参数）。
- **不在本批**：`crates/world/src/combat.rs:468-600` 的 `HAM_GUILD` 过滤、`enter.rs:269/343` 的
  `InGuildWarArea`/766 bit4、名字颜色。这些要世界层每实体知道行会/联盟/战态，写集独立，单列 Batch G5（R）。

---

## 5. 分批建议（写集 / 串行 / tier）

> 原则：按写集切，不按标题切（AGENTS §0b）。G1 可并行；G2/G3 因同触 `main.rs` 必须**串行同车道**；
> G4/G5 与既有 NPC/商店/战斗/进图车道有重叠，需先对齐 owner。

### Batch G1（L）— 帧编码层（纯函数）
- 写集：`mir2-rs/bins/game-svr/src/smframe.rs`（新 `sm_*` + 单测）。
- 内容：750/753/754/756/757/758/759/760/761/762/763/768/769/770/104 的编码；
  753/756 接收**已构造的文本**（面板文本构造器放 G2 的 `crates/world`，避免 G1 触 world）。
- 不接线（生产默认路径零变化）。tier=L。
- 可独立合入。

### Batch G2（L）— 会话态 + 9 CM 派发 + 750
- 写集：`mir2-rs/crates/world/src/guild.rs`（新增 `open_dlg_text`、notice 分段小函数；
  不含 IO）+ `mir2-rs/bins/game-svr/src/main.rs`（`SessionGround` 加 `GuildSession`；
  `sim_loop` 的 `else if` 加 `CM_GUILD*` 臂；登录帧注入 750；删成员/解散/登出的会话清空）。
  **建议把 9 个 handler 放到新文件 `bins/game-svr/src/guild_cmd.rs`**，`main.rs` 只留 3 处小 hunk，
  以避开 PL-1 车道在 `/e/tmp/pl1` 改的 `main.rs` P21/P32 区域。
- 依赖：G1。
- tier=L（纯状态 + 帧；不落盘、不抽 RNG）。**750 的登录出队序**若要在本批判，必须升 R；
  建议 G2 只判「有会时产 750 且尾字节正确」，出队序留给 G3 的窗口。
- **串行**：与 G3 同触 `main.rs`/`guild.rs` ⇒ G2 先合，G3 后合。

### Batch G3（R）— 持久化 + 10s/30s + 掉线
- 写集：`crates/world/src/guild.rs`（`run` 返回 save 请求；必要时加 `pending_save`）、
  `bins/game-svr/src/main.rs`（`EnterConfig` 加 `GuildCastleConfig`；启动加载；tick 门；断线 `DelHumanObj`；
  各变更点的 `store.save_*`）。
- 依赖：G2（同一批 dispatch 分支上补 IO）。
- tier=R：文件持久化 + 时序（10s/30s/到期）+ 出队序。必须真机窗口 + `MIR2_CLOCK_MS`。
- 串行：G2 → G3；不与攻城文件/城门车道共享 `GuildBase/**`（本车道只碰 `GuildBase`）。

### Batch G4（L）— 行会 NPC 入口（建会 / 宣战）
- 写集：`bins/game-svr/src/main.rs`（`CM_MERCHANTDLGSELECT` 的 NPC 回退）、
  `bins/game-svr/src/shop.rs` 或新 `npc_cmd.rs`（GuildOfficial 4 支）、可能 `crates/script`（base `UserSelect` 的 `GotoLable` 桥）。
- 必须先修的两个入口缺口：
  1. oracle 是 `FindMerchant(nParam1) ?? FindNpc(nParam1)`（`PlayObject.Operate.cs:118`），
     Rust 当前只认 `e.merchant.is_some()`（`main.rs:3849-3857`）⇒ **GuildOfficial（type=1，只有 `npc` 状态）
     永远打不开对话**。需加 `npc` 回退，并在 type=1 时走 GuildOfficial.UserSelect。
  2. 通用 `NormNpc.UserSelect` 的 `GotoLable` 桥未接（生产只接了 QFunction，`main.rs:1502`）；
     `npc::goto_label`（`bins/game-svr/src/npc.rs:246-266`）已具备 say 收集，可直接接。
- 分支：`@@buildguildnow`（`GuildOfficial.cs:112-178`）、`@@guildwar`（`:180-201`）、
  `@requestcastlewarnowN`（`:206-233`，N1 不可达；部署脚本实际写的是 `@requestcastlewarA`，
  与常量前缀不匹配 ⇒ 连分支都不命中）、`@@donate`（`SM_DONATE_OK` 764）。
- `request_guild_war` 已在 `crates/world/src/guild.rs:970-1105` 全链实现（含 N2 与「先扣钱后判掌门」）。
  建会链需新增 NPC 层纯函数（费用/物品/重名/已有会）到 `crates/world` 或 bins。
- tier=L（无 RNG、无时序；物品/金币是既有纯状态）。**串行**：与 NPC/商店车道（`openmir2-m3-npc-shop`）的
  `shop.rs`/`main.rs` 写集重叠 ⇒ 需 owner 排串行。

### Batch G5（拆分）
- **G5a（L）**：`bag.rs` 的 `has_guild/guild_rank_no`、`queryuser` 的 guild 字段、`SM_SENDUSERSTATE`。
  写集：`bins/game-svr/src/main.rs`（两处传参）+ `bag.rs` 测试。依赖 G2。
- **G5b（R）**：`crates/world/src/combat.rs` 的 `HAM_GUILD` 剔除（`combat.rs:512-535/576-600`）、
  进图 766 bit4 / `InGuildWarArea`（`bins/game-svr/src/enter.rs:269/343`）、名字颜色。
  理由：战斗目标集合是「战斗数值」且会改变命中流程 ⇒ R；写集与战斗/进图车道冲突 ⇒ **另行排期**，
  不并入行会会话层首合。

---

## 6. 判据草案（每批 ≥3 条构造级 + 牙）

> 「牙」= 具名断言；括号里是**必须让它变红**的改坏方式。所有构造级用例走生产函数，
> 不复制第二份实现。夹具层直接复用现有 `guild_castle_diff.py`（selftest 36/36）与
> `guild_castle_scenario.json`（9 case），不要再造比较器。

### G1 — 帧编码
1. `sm_guild_dlg_panel_bytes`：2 条公告 + 1 条战表 + 0 盟友、`rank=1` 与 `rank=99` 各一例，
   断言 753 的完整字节（`Recog=0,Param=0,Tag=0,Series=1` + `edcode::encode(GBK(text))`），
   含 `\r \r`、`<Notice>/<KillGuilds>/<AllyGuilds>` 与 `WarGuild { Guild = M2Server.Guild.GuildInfo,
   WarTick = .., WarTime = .. }` 字面量。
   **牙**：把 `Series` 从 1 改 0、把战表条目改成只打会名、或把 `\r \r` 改成 `\r\r` ⇒ 红。
2. `sm_guild_fail_codes_matrix`：754 纯头；758 `Recog∈{1..5}`；760 `Recog∈{1..4}`；761 `Recog∈{-2..-7}`；
   769 `Recog∈{0,-1,-2,-3,-4}`；断言 **771 在任何输入下都不产出**。
   **牙**：把 BreakAlly 失败改成发 771 ⇒ 红。
3. `sm_changename_750_tail`：有会 ⇒ 尾 `EncodeString("会名/封号")`；无会 ⇒ 纯头。
   **牙**：把 `/` 换成 `\` 或给无会路径也加尾 ⇒ 红。
4. `sm_member_list_5000_utf16`：构造 5001 个 UTF-16 单元（含代理对），断言截断点与 oracle 一致。
   **牙**：`encode_utf16().count()` 改为 `chars().count()` 且用代理对样本 ⇒ 红。
5. `sm_guildmessage_104`：`Recog=收件人 ActorId`、`Param=MakeWord(F,B)`、`Series=1`、尾 = GBK 文本。
   **牙**：`Recog` 用发送者 id ⇒ 红。

### G2 — 派发/会话
1. `guild_dispatch_9cm_matrix`：两假人（会长/成员）逐一驱动 9 个 CM，断言每个回包
   `(ident,recog,param,tag,series,tail)` 与状态变化。
   **牙**：删任意一条 match 臂 ⇒ 对应子断言红（`guild_castle_diff` 的 `frames_unclassified` 也会红）。
2. `guild_add_member_codes_and_success`：
   - 非掌门 ⇒ 758/1；目标离线 ⇒ 758/2；目标没面对 ⇒ 758/2；已在其他会 ⇒ 758/4；
     rank 数 = 400 ⇒ 758/4；`AllowGuild=false` ⇒ **758/5**；成功 ⇒ 757 + 目标 750（`会名/行会成员`）+
     目标加入 manager + `SS_207` 事件。
   **牙**：把 `AllowGuild` 检查移到 `AddMember` 之后（状态已变）或把 rank 数门改成成员数门 ⇒ 红。
3. `guild_del_member_and_self_disband`：删他人 ⇒ 759 + 对方 750 纯头 + 会话清空；自删 ⇒
   `CancelGuld` → `DelGuild`（会表少一会）+ `SS_206` + 759；非掌门 ⇒ 760/1；非成员 ⇒ 760/2。
   **牙**：删掉 `GuildMgr.DelGuild` 调用（会表仍存在）或把失败码写反 ⇒ 红。
4. `guild_notice_and_rank`：公告 `a\rb\r` ⇒ `notices=["a","b"]` + 753 重开；rank 成功 ⇒
   **ranks 变空（N3）** + 756；rank `-1` ⇒ 无帧；`-2/-3/...` ⇒ 761。
   **牙**：把 N3 修好（`ranks = parsed`）⇒ 成功用例红；把 `-1` 也发 761 ⇒ 红。
5. `guild_ally_break`：双方掌门面对面 ⇒ **769/-4**（EnableAuthAlly 恒 false）；断盟（联盟表空）⇒
   **769/0**；`771` 永不出现；`768/770` 永不出现。
   **牙**：把 `EnableAuthAlly` 默认 true 或把断盟失败改 771 ⇒ 红。
6. `guild_panel_no_guild`：无会 `CM_OPENGUILDDLG` ⇒ 754 纯头；`CM_GUILDMEMBERLIST` ⇒ **零帧**。
   **牙**：无会时回 753 或 756 ⇒ 红。

### G3 — 持久化/时序（R）
1. `guild_store_roundtrip_bytes`：影子目录（`E:/tmp` 下 copy）加载 → 建会 → 保存 → 重载；
   断言 `GuildList.txt` = GBK + **CRLF** + 每行会名；`<名>.txt` **不存在**（oracle `FileMode.Open` 吞异常）；
   `<名>.ini` 存在且 0 字节；已有会落盘后 `\r\n` 逐字节。
   **牙**：把 `\r\n` 改 `\n`、或给新会创建 `.txt` ⇒ 红。
2. `guild_run_10s_and_30s`：注入时钟；断言 `Run` 只在 `>10000ms` 边界触发；`WarTime` 到期在
   `(now-war_tick) > war_time` 的**严格大于**点；`CheckSaveGuildFile` 只在 `>30000ms` 后触发一次且
   `bo_changed` 复位。
   **牙**：把 `>` 改 `>=`、把 10s 改 1s、或不清 `bo_changed` ⇒ 红。
3. `guild_load_missing_file_and_war_quirk`：名单含一个缺 `.txt` 的会 ⇒ 该会被移除、名单被回写、
   `.ini` 已建；引用它的战表条目**不入表**；`WarTick=加载时刻`、`WarTime=0`（`s20` 残留）。
   **牙**：用初始名单快照做 `guild_exists`（而非存活集）、或把 `WarTime` 读成战表行的数字 ⇒ 红。
4. `guild_del_human_obj_compare_quirk`：成员名比断线名「大」⇒ 条目变空串；「小/等」⇒ 不动；
   并断言入口先走 30s 检查。
   **牙**：`==1` 改 `!=0` ⇒ 红。
5. `guild_login_750_order`（真机 R）：固定起点、已入会角色登录；断言 750 在 `46` 后、`654` 前，
   尾 = `会名/封号`；无会角色登录断言 **无 750**。
   **牙**：把 750 插到 `52` 之前（优先级写错）⇒ 红。此项用现有 `run_pair.ps1` 两相 pin 捕获帧序。

### G4 — NPC 入口
1. `merchant_dlg_select_falls_back_to_npc`：`CM_MERCHANTDLGSELECT` 打向 GuildOfficial actorId ⇒
   必须进入 GuildOfficial.UserSelect（不是「FindMerchant==null ⇒ 丢弃」）。
   **牙**：保留 `e.merchant.is_some()` 过滤 ⇒ 红。
2. `build_guild_six_branches`：空名 -4 / 已有会 -1 / 钱不足 -2 / 无号角 -3 / 重名 -4 / 成功 0；
   成功断言 `762`（不是 10303）、金币 -1_000_000、号角 -1、会表多一会、`MyGuild/rank=1`、`SS_205`。
   **牙**：把 `CheckItems` 挪到 `AddGuild` 之后（会已建但失败）或忘扣金币 ⇒ 红。
3. `guild_war_npc_gold_before_master`：目标不存在 ⇒ 文案「行会 X 不存在!!!」；钱不足 ⇒「你没有足够的金币!!!」；
   钱够但**非掌门** ⇒ 仍扣 30000（NPC 层先 `DecGold` 再 `ReQuestGuildWar`），且回掌门拒绝文案。
   **牙**：把 `DecGold` 移到掌门判定之后 ⇒ 金币差红。
4. `castle_war_label_n1_no_panic`：`@requestcastlewarnow0` 与部署实际的 `@requestcastlewarA` 都不 panic、
   不改金币/背包（oracle 分支不命中或 N1 抛异常被吞）。
   **牙**：让该标签真的扣祖玛头像/写 `AttackSabukWall.txt` ⇒ 红（当前不可达）。

### G5a（L）
1. `bag_need_6_7_60_70`：`has_guild/guild_rank_no/is_castle_member` 四组合与 Need 6/7/60/70 的真值表。
   **牙**：把 60 改成「有会即可」⇒ 红。
2. `userstate_guild_fields`：`CM_QUERYUSERSTATE` 回包的 20 字节会名 / 14 字节封号（GBK/ASCII 截断）
   与会话一致。**牙**：传空串 ⇒ 红。

### G5b（R）
1. `ham_guild_target_filter`：同会/盟友/战时不打，非同会打；含 `GuildWarArea` 分支。
2. `enter_766_bit4_and_home_random`：固定种子，攻城区/非攻城区进入，断言 766 bit4 与
   `HomeX-2+Random(5)`/`HomeX-4+Random(9)` 的抽取顺序（D-3 要求两条路径都覆盖）。
   **牙**：去掉 `Random(5/9)` ⇒ RNG 流位置红。

---

## 7. 风险 / 串行 / 坑（派工时逐条写进卡）

1. **`main.rs` 写集冲突**：PL-1 正在 `/e/tmp/pl1` 改 `main.rs` 的 P21/P32 区；本车道新代码尽量进
   `bins/game-svr/src/guild_cmd.rs`，`main.rs` 只留 `mod`、`SessionGround` 字段、else-if 臂、登录注入、
   断线/tick 五处小 hunk；合并前 rebase 并重跑四门禁。
2. **`GuildManager.run` 返回值**要改（30s 保存信号），会动 `crates/world` 既有单测（`guild.rs:1423-1442`）。
3. **N2/N3/N4/DelHumanObj/BanGuildChat/SendGuildMsg** 六个 oracle 怪行为必须照搬，不得顺手修：
   - N2 `AddWarGuild` 条件反置（`GuildInfo.cs:1039`）；
   - N3 `UpdateRank` 成功清空 `ranks`（`:970-971`）；
   - N4 `CheckGuildName` 漏比末字符（`M2Share.cs:536`）；
   - `DelHumanObj` `Compare(...)==1`（`:542`）；
   - `SendGuildMsg` 的 `if (guildMember.BanGuildChat)`（`:449`，名字与语义反向；默认 true）；
   - `SendGuildMsg` 用 `SendMsg(guildMember, ...)` ⇒ `SM_GUILDMESSAGE.Recog` 是**收件人自己**的 ActorId。
4. **联盟表恒空**、**`.bak` 永不落盘**、**新会 `.txt` 写不出去**、**`.ini` 只建空不写**——四处都是
   oracle 的「看起来该有但没有」；Rust 数据层已照搬，接线时别「补全」。
5. **`SS_205/206/207/208/211/212`**：单服部署下 `SendServerGroupMsg` 只发 plane/跨服通道
   （`src/GameSrv/Word/WorldServer.cs:885-895`），没有接收端 ⇒ **零客户端帧**。
   建议在 Rust 侧建模为事件（供日志/判据），不实现跨服发送；是否登记「单服不实现」需 owner 裁决
   （audit #19）。
6. **750 的登录出队序**是唯一必须占窗口的 L 批子项；不想占窗口就把它拆到 G3（R），G2 只判帧字节。
7. **时钟域**：G3 必须用 `MIR2_CLOCK_MS` 墙钟量级；否则 `dw_save_tick=0` 的首次 30s 门与 C# 分叉。
8. **GuildOfficial 对话入口**（G4）是建会链的硬前置：不修 `FindMerchant ?? FindNpc`，即使 9 CM 全接好，
   生产也无法产生第一个行会（部署 `GuildBase/GuildList.txt` 为 0 字节、`Guilds/` 不存在）。
9. **测试不得起栈**：构造级判据全部走 `sim_loop` 同级的生产函数 + 假 `gates`（现有多数用例的 rig 方式）；
   真机项只在 G3/G5b 的窗口里跑。

---

## 8. 证据与可复算（只读）

- 仓库快照 `git status --porcelain` = 空；本车道未写仓库、未改 `E:/MirServer`、未起进程。
- 输入哈希（sha256）：
  - `7271fea10500f1931812f34f3618fd3d6a3a2a14f045b498f9cb45408f3e22df  mir2-rs/tests/parity/m3/audit-uncovered-surfaces-20261019.md`
  - `6806836cc68ad19a89bc5efd262d76c363932584027c3722538d92ae80c38351  mir2-rs/tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md`
  - `f27c6f8f77ec2e5a308ad0c381e50cb2b784c6df8c6b4640415cba6ddafacbea  mir2-rs/crates/world/src/guild.rs`
  - `c428210964f184cc78e893a18f73a2562bee1794e3921d62034dccf19775fa28  mir2-rs/crates/data/src/guild.rs`
  - `9b04aea209c8f380fe0d7523c8b0a2cf89520c5f5dffe6bf1e2fb4e0b38bbe77  mir2-rs/bins/game-svr/src/smframe.rs`
- 复核命令（只读）：
  ```bash
  grep -n "CM_GUILD" mir2-rs/bins/game-svr/src/main.rs          # 无输出
  grep -n "GUILD" mir2-rs/bins/game-svr/src/smframe.rs           # 无输出
  grep -rn "GuildStore\|load_guild_info" mir2-rs --include=*.rs | grep -v crates/data/src/guild.rs
  grep -n "has_guild: false\|guild_rank_no: 0" mir2-rs/bins/game-svr/src/main.rs
  grep -n "BanGuildChat\|EnableAuthAlly" src/M2Server/Guild/GuildInfo.cs src/M2Server/Player/PlayObject.cs
  grep -n "GuildMgr.Run" src/GameSrv/Word/Threads/SystemProcessor.cs
  ```
- 残留物：仅本文件 `E:/tmp/lane3-guild-brief.md`；无 worktree、无编译目录、无会话目录。

---

## 9. 建议派卡顺序（一句话版）

1. `openmir2-m3-guild-frames`（G1，L，可立即开）。
2. `openmir2-m3-guild-state` 第②片（G2，L；引用既有卡号，不新排队）。
3. `openmir2-m3-guild-file-store` 第②片 + `openmir2-m3-guild-war` 时序（G3，R；等窗口 + `MIR2_CLOCK_MS`）。
4. `openmir2-m3-guild-official-label`（G4，L；与 NPC/商店车道串行）。
5. `openmir2-m3-guild-downstream`（G5a 先做，G5b 另行排期）。
