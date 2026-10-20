# Lane B — 行会 G3（持久化 + 10s/30s + 掉线）实施简报 v7

> **本文完整取代 `design-guild-g3-persistence-v6-20261020.md`。** v6 的正文与第二轮复审 12 条已并入；
> 本版并入**第三轮复核** N-1..N-5（落点见 §0.5 与文末「v7 变更记录」）。v6 只保留顶部一行指引。
v2 / 首版仅作背景阅读（时钟域、Compare 语义、窗口 A/B 授权边界的来源），判据以本文为准。

- 基线：`35c039c8d`（master，2026-10-20）。
- 产出线：文档 worktree `/e/tmp/g3v4`，分支 `g3v4`（本文件是唯一改动）；**实现线另起 worktree**，不得在本分支写代码。
- 只读取证：`src/**`、`mir2-rs/**`（oracle 与实现骨架均未修改）。
- **oracle 路径注意**：`PlayObject.*.cs` 在 `src/M2Server/Player/`；`WorldServer.cs` / `SystemProcessor.cs` / `UserProcessor.cs` 在
  `src/GameSrv/Word/`；`GuildInfo.cs` / `GuildManager.cs` 在 `src/M2Server/Guild/`；配置模型在 `src/Modules/SystemModule/Conf/`。
  v3 与评审文本里的 `src/GameSrv/PlayObject.Message.cs` 不存在（真实路径见上）。

---

## 摘要（≤800 字）

v7 = v6 的判据面 + 第三轮复核 5 条。核心：① **N-1**：A5 补「`sg.bo_soft_close = true;` 在早退 `continue` **之后**、`guild_closed` 之前」
的顺序断言，并锚 `closed_event_is_current` 的早退位置；同时把整段抽成可测的 `prepare_closed_guild`（arm-level），新增 **W4-14**
证明「陈旧 Closed 后新会话 flags 不变、当前 Close 保留 `bo_reconnection`」，并补 `bo_reconnection: sg.bo_reconnection,` 断言。
② **N-2**：helper 改比 `(gate_idx, socket)`（降低跨网关误匹配）；早退挪到 `kick_pending.retain`（15406）**之后**，消除 v6 新增的
「陈旧 Close 不清理 pending kick」行为；`GateEvent::Closed` 不带 game `session_id` ⇒「同网关 socket 号复用」登记为 T-101 残余；
T-101 改 `tier=R` + `window_required: true`。③ **N-3**：m23 改为「删早退 ⇒ **W4-14 ①** + A5 红」（v6 只咬纯 helper ⇒ 无牙）。
④ **N-4**：T-96 改「**启动时** `GetPlayObject` 为空 ⇒ null；**重载时**在线成员被重绑」。⑤ **N-5**：T-102 补 `tier_why` 边界。
其余判据不变。

---

## 0. 变更总览

### 0.1 评审 8 点（v3 已落，v4/v5 保留）

| # | 评审点 | 落点 | 牙（哪条变异必红） |
|---|---|---|---|
| 1 | 断线 104 退出提示被漏 | §2 规格；W4-2/4/5 | W4-m9/m12/m13；104 编码器既有单测 |
| 2 | `CM_SOFTCLOSE` 生命路径未入清单 | §3；W4-4/8a；`guild_done` | W4-m5b/m18 |
| 3 | W2 用例 C 与 10s 门矛盾 | W2-C：每步 `*run_tick = now - 10001` | W2-m2 |
| 4 | W1 顺序牙与字母序重合 | W1 fixture 改 `B,A,C` | W1-m8 |
| 5 | W1 只测 4 个段头 | W1-6b + W3-0 + W3-1038b | W3-m8/m9 |
| 6 | W3 缺 add_guild 用例、m7 不可执行 | W3-0；W3-m7 | W3-m7 |
| 7 | W3 缺「自删解散漏 save_one」变异 | W3-m3b | 文件仍含旧 `#1/+GM` |
| 8 | W4「下一次保存写出 `+\r\n`」是错的 | §6 W4 断言 W4-1/3 + 说明 | W4-m3（清在存前）/ m4（不存） |

### 0.2 评审 C1/C2/C3（v4 新增）

| # | 评审点 | v4/v5 落点 | 牙 |
|---|---|---|---|
| 9 | **C1（模型错误）** 104 与清槽被拆成两个源 | §2.1a「一个源」；§3；W4 伪代码（`my_guild` 单变量 + 写回会话）；W4-6a/6b/6c/6d；m2 重定义 | W4-m14（不做覆盖）⇒ W4-6a 红；W4-m15（覆盖只给 104、清槽另读会话）⇒ W4-6b 红；W4-m16（路径 B 也覆盖）⇒ W4-4b 红 |
| 10 | **C2（接线缺口）** 离场者排除只由测试闭包实现 | §2.7；`guild_broadcast_104(..., exclude)`；W4-11（生产形状 `online`）；两条锚点（A1 调用点 / A2 排除行 / A3 闭包） | W4-m17（删排除行）⇒ W4-11 红（须**不**先摘 `online`，见 §6 W4-11 注） |
| 11 | **C3（覆盖缺口）** `SS_208` 无断言 | §2.4（字段/拼装/门）；W4-10a/10b/10c/10d | W4-m20（ident/`server_index`/分隔符写错）、m21（只在有帧时产）、m22（路径 B 也产） |

> 行 1、2 的语义被 C1 修订：104 的「按名查会」与清槽的「按会话」是**同一个会话字段的两次读**，不是两个源。相应期望见 W4-6。

### 0.3 评审 v5 的 C/I/M 逐条落点

| 评审号 | 改法 | 位置 |
|---|---|---|
| C-1 | `path_a` 单变量；覆盖写、104、SS_208 全包在 `if path_a` 内；check_save/清槽不分路径 | §6 W4 伪代码；m22 |
| C-2 | 调用前快照 `ban: HashMap<String,bool>`；`guild_recipients(online, world, &ban)` | §2.7、§6 W4 生产闭包；W4-11 |
| I-1 | 收件人 = 缓存 `PlayObject`（T-96）；夹具假设 + 窗口 A 观察 | §2.1a、§9-18、§10.2 |
| I-2 | 撤回「游离对象」；T-97；W4-12 解散后断线 | §9-19、W4-12 |
| I-3 | W4-6c 钉 manager 顺序 `[H,G]` + 前置断言 | W4-6c |
| I-4 | W4-8a 改**路径 A 双调用**；CM_SOFTCLOSE 归 W4-4a/8b | W4-8a、m18 |
| I-5 | W4-4b = `true/true`；W4-10b = `true/true` 与 `false/false` | W4-4b/10b |
| I-6 | W1-6b 拆 `GuildConfig`（`main.rs:14403` 覆盖）与 `GuildMsgCfg`；写集补 | W1-6b、§7 |
| I-7 | 生产锚点 A4（startup / step+`guild_run_tick` / `handle_guild_cmd(..., world.clock_ms as i32)` / 四条 `on_disconnect` 早于 `ground_sessions.remove`） | 锚点 A1/A4 |
| I-8 | 统一「会话条目存在但 `guild_name=None`」vs「无会话条目」 | W4-6a/6d |
| I-9 | 窗口 A 增 104/SS_208 真机项 | §10.2 |
| M-1..M-14 | 逐条见文末「v5 变更记录」 | — |

### 0.4 第二轮复审（v6）落点

| 清单 | 改法 | 位置 |
|---|---|---|
| 1 | §2.7 闭包改 `guild_recipients(&online, &world, &guild_ban)`（与 C-2 一致） | §2.7 |
| 2 | W4-12 改 GM `del_guild` 前置；T-97 改述 bound/unbound 两种 `BackupGuildFile` 结果；删「1039 自删后其它在线成员」 | W4-12、§9-19 |
| 3 | `closed_event_is_current` helper + `Closed` **整臂早退**；W4-13 驱动 helper；T-101 残余缩小 | W4-13、A5、§9-20、m23 |
| 4 | A4④ 限定到 `handle_guild_cmd(` 末参；A1 拆「三条路径 + `guild_softcmd` 无 remove」 | A1/A4 |
| 5 | 新增锚点 A5（N-7）：两处置位 + 早退行 | A5 |
| 6 | W4 fixture 头写默认会话 | W4 断言头 |
| 7 | T-96 `:774-778` 改述为 `UpdateRank` 丢弃；补 `ReloadAllGuild` 重绑 | §2.1a、§9-18 |
| 8 | §9-1 战争到期改「第 n 跳晚 150n」 | §9-1 |
| 9 | §9 标题/9-0 标「待写入、尚未写入」+ 让号规则 | §9、§9-0 |
| 10 | §2.1a 表格与 W4-4b/6b/12 的成员表措辞统一 | §2.1a、W4 |
| 11 | §2.5 `drain_soft_close` 行加「若无既有 `BoReconnection`」 | §2.5 |
| 12 | T-97 与 T-96 同为 `tier=R`；T-101/T-102 补 `tier`/`tier_why` | §9 |

### 0.5 第三轮复核（v7）落点

| 编号 | 改法 | 位置 |
|---|---|---|
| N-1 | `prepare_closed_guild` 抽出；A5 加「置位在早退之后」顺序断言 + `bo_reconnection: sg.bo_reconnection,`；新增 W4-14（arm-level） | A5、W4-14、§6 helper |
| N-2 | helper 比 `(gate_idx, socket)`；早退挪到 `kick_pending.retain`（15406）之后；socket 复用登记为 T-101 残余；T-101 → `tier=R` + `window_required` | A5③、§9-20 |
| N-3 | m23 改为「删早退 ⇒ W4-14 ① + A5 红」 | m23 |
| N-4 | T-96 改「启动时 null / 重载时重绑」 | §9-18 |
| N-5 | T-102 补 `tier_why` 边界 | §9-21 |

---

## 1. 对 v2 的关键订正（v3 表 + v4 增补）

| # | v2 | v3/v4/v5 订正 |
|---|---|---|
| 1-1 | W4 只列 `Closed`/`drain_kicks`/`drain_soft_close` 三处，断线只做 check_save+清成员 | **四处**：加 `CM_SOFTCLOSE`；`Closed`/账号顶号还要发 **104 退出提示**（§2）。四处都必须「只跑一次」。 |
| 1-2 | W4 断言「下一次保存写出 `+\r\n`」 | **错**。`CheckSaveGuildFile`（`GuildInfo.cs:525-533`）只置 `BoChanged=false` + 落盘，**不更新 `DwSaveTick`**；`DelHumanObj`（`:534-550`）清槽**不置 `BoChanged`**。⇒ 断线后文件仍是旧体（含 `+b`），清槽在内存中，**此后没有自动保存**会把它写出。要观察 `+\r\n`，再走一条 `SaveGuildInfoFile` 生产路径（1038/1039/1040/1041 任一），并同时断言 `bo_changed=false`、`dw_save_tick` 不被 check_save 改动。 |
| 1-3 | W2 用例 C：`now=150/151` 直接 `step` | 10s 门 `now - run_tick > 10000` 不通过 ⇒ `guilds.run` 根本不会跑。每步前显式 `*run_tick = now_tick - 10_001`（§6 W2-C）。 |
| 1-4 | W1 fixture `A,B,C`，断言「名单序非字母序」 | `[A,B]` 与字母序重合，无牙。改 `GuildList.txt = B\r\nA\r\nC\r\n` ⇒ 存活表 `[B,A]`、回写 `B\r\nA\r\n`（§6 W1）。 |
| 1-5 | W1 断言「六键段头来自 `from_strings`」 | 六键不是同一类：4 个 **段头**（`GuildNotice/GuildWar/GuildAll/GuildMember`）进 `GuildFileSections`；2 个 **封号**（`GuildChief/GuildMemberRank`）进 `GuildConfig`。分段断言见 W1-6b / W3-0 / W3-1038b。 |
| 1-6 | W3 m7「新建会路径创建 `<名>.txt` ⇒ 红」 | 没有对应用例 ⇒ 不可执行。补 **W3-0 add_guild**（世界层 `add_guild` + 生产 `save_one`/`save_list` 组合，照 `GuildManager.AddGuild` 尾部）。（NPC `GuildOfficial.ReQuestBuildGuild` 尚未移植，登记 §9-14。） |
| 1-7 | W3 自删解散只有 m3（漏 `save_guild_list`） | 补 **m3b**：漏 `save_one` ⇒ `<名>.txt` 仍是旧 `#1/+掌门` 体 ⇒ 红。 |
| 1-8 | 104 未建模 | 补 §2：条件、文本、帧形、收件人、前缀/颜色、与清槽的先后、`SS_208`。 |
| 1-9 | `[Setup] ShowPreFixMsg` 未列入行会写集 | `config.rs` 已解析（`GateConfig.show_pre_fix_msg`，`config.rs:301-304/1102`），G3 **复用**到 104；`[String] GuildMsgPreFix` 必须新解析（`strings.rs`）。 |
| 1-10 | 收件人未过 `BanGuildChat` 门 | `GuildInfo.SendGuildMsg` 只在 `guildMember.BanGuildChat == true` 时发（字段名反向，默认 true；`PlayObject.cs:1007`）。W4-9 钉住。 |
| 1-11 | `on_disconnect` 在 `Closed` 的调用位置：party 块之后 | oracle 的 104 在 `PlayObject.Run()` 尾段（`PlayObject.Message.cs:367-389`），清槽在 `Disappear()`（`:2144-2147`）。**104 是帧**，它与其它帧的相对序**可观测**（M-13 订正）：oracle 序 = 104（本轮 `Run`）→ party 退组帧（下一轮 `Disappear`，`:2139-2143`）→ 交易取消帧（再后 `DealCancelA`，`WorldServer.cs:712`）。v5 规定 `on_disconnect` 放在 `deal_cancel_between` **之前**、**有活会话时**才调 ⇒ 104 先于这两类帧。*（Rust 内 deal_cancel 与 party 的相互序与 oracle 不同，属 ⑦ 既有口径，不在 G3。）* |
| 1-12 | 同一会话可能被清两次 | `guild_done` 放在 **`GuildSession`**（不是 `SessionGround`：`GuildSessions` trait 只暴露 `GuildSession`，模型测试用 `HashMap<String, GuildSession>`）；**外加**「无活会话 ⇒ 完全 no-op」门（C1 波纹：只靠 `guild_done` 挡不住 `drain_*` 已 `ground_sessions.remove` 之后到来的 `Closed`）。两门都在**第 1 步（覆盖）之前**判。 |
| **1-13** | 104 与清槽「两个源」 | **C1**：一个源 `my_guild`（= 会话 `guild_name`）；路径 A 用 `member_of_guild` 覆盖并写回，路径 B 不覆盖。三条可观测后果见 §2.1a。 |
| **1-14** | 收件人排除交给测试闭包 | **C2**：`guild_broadcast_104(..., exclude: Option<&str>)`，`on_disconnect` 传 `Some(ctx.chr_name)`；生产形状的 `online` 必须由 W4-11 覆盖。 |
| **1-15** | `SS_208` 只落事件 | **C3**：事件 `ServerGroupMsgPayload { ident, server_index, payload }`，W4-10a..d 逐字段 + 逐条件断言。 |
| **1-16** | 104/SS_208 由 `my_guild` 门控 | **C-1**：104/SS_208 只在**路径 A** 内产生；路径 B 即使 `my_guild` 非空也只做 check_save+清槽。 |
| **1-17** | 生产闭包直接捕获 `ground_sessions` | **C-2**：先快照 `ban: HashMap<String,bool>`；闭包只捕获 `&online`/`&world`/`&ban`（否则与 `&mut ground_sessions` 冲突，E0502）。 |
| **1-18** | 收件人 = 所有在线成员 | **I-1**：oracle 是缓存的 `GuildMember.PlayObject`（T-96）；本仓按 `online` 解析，差异登记 + 夹具假设 + 窗口观察。 |

---

## 2. 断线 104 退出提示规格

### 2.1 oracle 出处与条件

104 块在 **`PlayObject.Run()` 尾段**（`src/M2Server/Player/PlayObject.Message.cs:367-389`）；`Run()` 由
`WorldServer.ProcessPlayObjectData()` 在 `!playObject.Ghost` 且 `RunTick` 到期时调用（`src/GameSrv/Word/WorldServer.cs:645-648`、`:697`）：

```csharp
if (BoEmergencyClose || BoKickFlag || BoSoftClose) {          // :367
    if (SwitchData) { ... }                                    // 换服搬家，G3 不涉及
    MakeGhost();                                               // :375
    if (BoKickFlag) SendDefMessage(SM_OUTOFCONNECTION, 0,0,0,0);
    if (!BoReconnection && BoSoftClose) {                       // :380
        MyGuild = SystemShare.GuildMgr.MemberOfGuild(ChrName);  // :382 覆盖会话字段（可为 null）
        if (MyGuild != null) {                                  // :383
            MyGuild.SendGuildMsg(ChrName + " 已经退出游戏.");     // :385
            SystemShare.WorldEngine.SendServerGroupMsg(Messages.SS_208, M2Share.ServerIndex,
                MyGuild.GuildName + '/' + "" + '/' + ChrName + " has exited the game.");   // :386
        }
    }
}
```

要点：

1. **触发式**：`!BoReconnection && BoSoftClose`（不是「只要断线就发」）。
2. **覆盖会话字段**：`:382` 赋值给 `MyGuild`（`PlayObject` 的字段），不是局部变量——这是 C1 的全部要点。
3. **`MakeGhost()` 之后**：`SendMsg` 有 `!Ghost` 门（`src/M2Server/Actor/BaseObject.Message.cs:144`，判的是**收件人** `this.Ghost`），
   所以**离场者自己收不到**（它已 Ghost）；其他在线成员正常收。
4. **扇出早于清槽**（确定序，非竞态）：104 在 `Run()`（`:697`）里；本轮 `if (!Ghost)` 已在 `:648` 判定过，`else`（`:707-716`）
   本轮不会对刚 Ghost 的对象再判 ⇒ `PlayObjectList.Remove` + `Disappear()` 落在**下一轮**。`UserProcessor` 每 50ms 跑一次
   `ProcessHumans`（`src/GameSrv/Word/Threads/UserProcessor.cs:27-36`），单线程。⇒ 旧 §9-9 的「线程竞态」表述作废。

### 2.1a 一个源（C1）

`Disappear()`（`PlayObject.Message.cs:2129-2150`）用的是**同一个** `MyGuild`：

```csharp
if (MyGuild != null) { MyGuild.DelHumanObj(this); }   // :2144-2147
```

`DelHumanObj`（`GuildInfo.cs:534-550`）先 `CheckSaveGuildFile()`（`:525-533`）再清第一个
`string.Compare(member.MemberName, playObject.ChrName, OrdinalIgnoreCase) == 1` 的槽（`:542`）。
**`DelHumanObj` 全仓只有一个调用点**（`Disappear`，grep 已核）⇒ 清槽的「会」只可能来自 `MyGuild`。

于是只有两条路径：

- **路径 A**（`!BoReconnection && BoSoftClose`）：`MyGuild := MemberOfGuild(ChrName)`（`GuildManager.cs:112-122`，首个 `IsMember` 命中；
  `IsMember` 是大小写敏感 `==`，`GuildInfo.cs:161-179`）**覆盖**，可为 null ⇒ 104/SS_208 与后续 check_save/清槽**共用这一个值**；
- **路径 B**（`BoReconnection` 或 `!BoSoftClose`）：**不覆盖** ⇒ 清槽用会话缓存里的 `MyGuild`。

v3 把「104 按名查 / 清槽按会话」拆成两个源，是错的。三条可观测后果（全部进 W4）：

| 情形（路径 A） | oracle 结果 | v3 的错误期望 |
|---|---|---|
| 会话条目存在但 `guild_name == None`，manager 有会 | 覆盖成 Some ⇒ **发 104/SS_208 + check_save + 清槽** | 完全 no-op（W4-6 旧文） |
| 会话 Some(G)，manager 无会 | 覆盖成 **null** ⇒ **不发 104、不 check_save、不清槽**（G 不动） | 按会话清 G、按名不发 104 |
| 会话 Some(G)，manager 有会 H | 104 发进 H、SS_208 用 H、清 H，G 不动 | 清 G、104 发进 G（两个源各说各话） |

Rust 建模（`guild_runtime::on_disconnect`）：

- 单变量 `my_guild: Option<String>`，初值取会话 `GuildSession.guild_name`；
- 路径 A 时 `my_guild = guilds.member_of_guild(chr_name).map(str::to_string)` 并**写回** `GuildSession.guild_name`（oracle 覆盖的就是字段本身）；
- 之后 104、`ss208`、`check_save_guild_file`、`del_human_obj_member` **全部**用 `my_guild`；**不得**再读一次会话作为清槽源；
- `my_guild == None` ⇒ 三者都不做（104 也不发）。

`GuildSession.guild_name` 是 G2 的会话缓存（`guild_cmd.rs:86-97`，登录物化 `guild_cmd.rs:144-158`），不是真源——与 oracle「manager 是真源、
`MyGuild` 是缓存」的口径一致。

**收件人 ≠「所有在线成员」（I-1 / T-96）**：`SendGuildMsg` 读的是 `RankList` 里**缓存的** `GuildMember.PlayObject`
（`GuildInfo.cs:444-448`，`null` 跳过）。**有效绑定点只有三处**：`LoadGuild`（`:333-338`，启动时 `GetPlayObject` 为空 ⇒ `null`）、
`SetGuildInfo`（`:474-478`）、`AddMember`（`:645-649`）。**`:774-778` 不是绑定点**——那是 `UpdateRank`（`:708` 起）的解析循环，
临时表随后被 `RankList = rankListTmp`（`:966-971`，空表）丢弃（未变更路径 `:817-822` 也 `ClearRankList`）；**登录时的重绑被注释掉**
（`:495`、`:980`）。另 GM **`ReloadAllGuild`**（`ReloadAllGuildCommand.cs:21` → `GuildMgr.LoadGuildInfo()`，`GuildManager.cs:78`）
走 `LoadGuild` ⇒ **重载会对当前在线成员重新绑定**。⇒ 重启后从文件加载的成员 `PlayObject == null`，**收不到任何行会消息**
（104 / WarEnded / 聊天），直到被重新 `AddMember` 或 GM 重载。
本仓以 `online` 解析收件人（= 修掉该缓存 bug）。按硬约束③，这是**差异**，登记 **T-96**（§9-18）。
**夹具假设**：W2-C/W4 的收件人断言只覆盖「本进程内 `add_guild`/`AddMember` 进入」的成员（= oracle 已绑定）；
从文件加载的成员不作收件人断言。窗口 A 必须用本会话内 live 建会/加人的成员，并记录实际收件人集（§10.2）。

### 2.2 `SendGuildMsg` 的文本与门

`GuildInfo.cs:431-456`：

```csharp
if (SystemShare.Config.ShowPreFixMsg) sMsg = SystemShare.Config.GuildMsgPreFix + sMsg;   // :435-438
for (int i = 0; i < RankList.Count; i++)                        // 逐 rank、逐 member 的固定序
    { IPlayerActor guildMember = guildRank.MemberList[j].PlayObject;
      if (guildMember == null) continue;                        // 离线成员：PlayObject 为空
      if (guildMember.BanGuildChat)                             // 反向命名：true = 允许收
          guildMember.SendMsg(guildMember, RM_GUILDMESSAGE, 0, FColor, BColor, 0, sMsg); }  // :451
```

- 文本：`ChrName + " 已经退出游戏."`（名字与「已经」之间**一个半角空格**）。
- 前缀：`ShowPreFixMsg` 为真时 `GuildMsgPreFix + 文本`，**无空格**；`GuildMsgPreFix` 缺省 `〖行会〗`
  （`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1843`），来自 **String.conf `[String]`**（`StringConf.cs:38`）。
- 颜色：`[Setup] GuildMsgFColor/GuildMsgBColor`，缺省 `0xDB/0xFF`（`GameSvrConf.cs:1542-1543`，`ServerConf.cs:335-336`）。
- 收件门：`BanGuildChat == true`（**反向命名**，`PlayObject.cs:1007` ctor 置 true = 允许；本仓 `GuildSession.ban_guild_chat` 同义）。
- 空文本不发（`SendSocket(defMsg, sMsg)`，`PlayObject.Socket.cs:111-114`）；本路径文本恒非空。另 `:107-110` 有**收件人** `OffLineFlag` 门
  （挂机中除 `SM_OUTOFCONNECTION` 外整帧丢弃）——本仓未建模，登记 §9-16。
- **发送口径（M-11）**：Rust 立即 `SimToGate::SendFrame`（与 party/NPC 帧同口径）；oracle 是 `SendMsg` 入 `MsgQueue`、下一次
  `ProcessBufferSend` 才写 socket。同一广播内 rank/member 序一致；跨帧类型**且跨收件人**的相对序不可观测（同收件人同时看到 104 与
  party/交易帧时的固定序见 §1-11）。登记 §9-21。

### 2.3 帧形（复用现有编码器）

`guild_frames::sm_guild_message_104(recipient_actor_id, f, b, text)`（`guild_frames.rs:278-296`）：

- `Recog = 收件人自己`的 ActorId（`SendMsg(baseObject=guildMember, …)` ⇒ `processMsg.ActorId = baseObject.ActorId`，`PlayObject.Message.cs:1720-1722`）；
- `Param = MakeWord(GuildMsgFColor, GuildMsgBColor)`（低字节 F、高字节 B）；
- `Series = 1`；尾 = `EncodeString(GBK 文本)`；空尾整帧不发。
- 既有用例 `sm_guild_message_104_recog_is_recipient_and_series_one` 已锁编码；W4 锁的是**接线与收件人集**。

### 2.4 `SS_208`（C3）

调用：`SendServerGroupMsg(Messages.SS_208, M2Share.ServerIndex, MyGuild.GuildName + '/' + "" + '/' + ChrName + " has exited the game.")`
（`PlayObject.Message.cs:386`）。

`SendServerGroupMsg`（`src/GameSrv/Word/WorldServer.cs:885-895`）把它拼成三段：
`nCode + "/" + nServerIdx + "/" + sMsg`，再交 `PlanesServer.SendServerSocket`（`src/Modules/PlanesSystem/PlanesServer.cs:120-136`；
`ServerIndex == 0`）或 `PlanesClient`，最终 `HUtil32.GetBytes`（GBK）。单服无接收端（同 `SS_206/207`）。

Rust 事件必须**逐字段**可断言，因此**不要把三段拼成一个串**：

```rust
pub(crate) struct ServerGroupMsgPayload {
    pub ident: u16,          // messages::SS_208（protocol/src/messages.rs:598）
    pub server_index: i32,   // M2Share.ServerIndex（= enter_cfg.server_index as i32，main.rs:12609）
    pub payload: String,     // "<GuildName>//<ChrName> has exited the game."（= oracle 的 sMsg 实参）
}
```

- 线上字节（将来接多服时）= `format!("{}/{}/{}", ident, server_index, payload)` 再 GBK；G3 不发送，只落事件/日志。
- `describe()` = `SS_208/{server_index}/{payload}`。
- **触发门与 104 相同**（路径 A 且 `my_guild == Some`），**与 104 实际发出多少帧无关**：`if (MyGuild != null)`（`:383`）
  之外没有第二条门——会里只剩离场者本人时 104 一条都发不出，但 `SS_208` 照发（W4-10d 钉住）。

### 2.5 每条宿主路径的输入（条件表）

| 宿主路径 | oracle 置位 | 覆盖 `MyGuild`？ | 104 / SS_208？ | 依据 |
|---|---|---|---|---|
| `GateEvent::Closed`（网关 socket 关闭，已 `LoginNoticeOk`） | `CloseUser`：`if (!OffLineFlag) BoSoftClose = true` | **是** | **是** | `ChannelMessageHandler.cs:388-394` |
| 同上但 `LoginNoticeOk == false`（未发 1018） | 同上 | **否**（走 `RunNotice`，无 104 块） | **否**；清槽仍发生（用会话值） | `WorldServer.cs:648-653`、`PlayObject.cs:1079-1091` |
| `drain_kicks`（`KickOnlineUser` / 脚本 `KICK` / 重复进世界踢旧） | 只置 `BoKickFlag` | 否 | **否**（`BoSoftClose=false`） | `WorldServer.cs:1052-1063`、`PlayObject.cs:1082-1090` |
| `drain_soft_close`（账号级顶号 `KickUser`） | `BoEmergencyClose=true, BoSoftClose=true`（`BoReconnection` 不动；**若无既有 `BoReconnection`** 才走路径 A——若该会话曾 `CM_SOFTCLOSE`，该位已为真） | 是（同前限定） | 是（同前限定） | `TCPNetChannel.cs:261-270` |
| `CM_SOFTCLOSE(1009)` | `BoReconnection=true, BoSoftClose=true`；`wParam==1` 再置 `BoEmergencyClose` | 否 | **否**（`!BoReconnection` 假） | `PlayObject.Message.cs:874-884` |
| （**未建模**）挂机超时 | `OffLineFlag=false; BoSoftClose=true` | 是（若无 `BoReconnection`） | **是** | `PlayObject.Message.cs:235-238`；置位脚本 `ExecutionProcessingSys.cs:2716-2731` ⇒ 登记 §9-16 |
| （**未建模**）账号到期 | `BoEmergencyClose=true`（仅） | 否 | 否（`BoSoftClose` 假） | `PlayObject.Message.cs:37-52` ⇒ 登记 §9-17 |

`Closed` 与 `drain_soft_close` 的 Rust 宿主必须**先**按 oracle 把会话的 `bo_soft_close` 置真（`CloseUser`/`KickUser` 语义），再交
`on_disconnect`；`drain_kicks` 保持 false；`CM_SOFTCLOSE` 已在命令里置 `bo_reconnection=true`。
**禁止**把「是否发 104」写成路径分支常量——统一由这两个 flag 判（变异才可咬）。

### 2.6 执行序（Rust）

`on_disconnect` 内固定序：**(0) 门 → (1) 解析/覆盖 `my_guild` → (2) 104 帧 + SS_208 → (3) check_save（先落盘）→ (4) 清槽**。
与 oracle 等价：oracle 的 104 在 `Run()`、check_save/清槽在下一轮 `Disappear()`；Rust 压进同一次调用，跨轮相位差 ≤50ms（§9-1）。

### 2.7 收件人排除（C2）

oracle 排除离场者靠三件事，缺一不可：`guildMember.PlayObject != null`、`!Ghost`（`BaseObject.Message.cs:144`）、
`guildMember == null` 的跳过分支。Rust 侧对应：

1. `recipients(name)` 由**生产**闭包 `guild_recipients(&online, &world, &guild_ban)`（`main.rs`；`guild_ban` = 调 `on_disconnect` 前的快照，见下）实现：
   `guild_cmd::find_play_object(online, world, name)`（已含「在线 + 实体存在 + `!ghost`」三重门，`guild_cmd.rs:182-194`）
   → `(actor_id, ban_guild_chat)`；
2. **`on_disconnect` 内显式排除 `chr_name`**：`guild_broadcast_104(..., exclude: Some(ctx.chr_name))`。

第 2 条是 C2 的实质，但**只有 `Closed` 臂必须靠它**（M-1 订正）：`Closed` 在 `online_take`（`main.rs:15420`）与 `world.disconnect` 之前调
`on_disconnect`，离场者仍在 `online` 且实体 `ghost == false` ⇒ `find_play_object` 会返回它；`drain_kicks`/`drain_soft_close` 则在
`online_take`（`main.rs:12120/12252`）**之后**、`world.disconnect`（`12139/12267`）**之前**调，离场者已不在 `online`。
显式排除对四条路径统一生效（对 drains 冗余但无害），是 `Closed` 的唯一防线。

**借用约束（C-2）**：生产闭包若捕获 `&ground_sessions`，就会与 `on_disconnect(..., &mut ground_sessions, ...)` 同时借用同一张表（E0502）。
宿主必须在调用前先快照 `HashMap<String, bool>`（角色名 → `ban_guild_chat`），闭包只捕获 `&online`、`&world`、`&snapshot`。
oracle 的 `MakeGhost()`（`:375`）在 `SendGuildMsg` 之前，所以离场者永远收不到；Rust 用 `chr_name` 精确排除等价（`MemberOfGuild`/`IsMember`
是大小写敏感 `==`，成员槽名与 `chr_name` 逐字相同）。**不选**「先做 Ghost」路线：本仓 `world.disconnect` 直接删实体、无 `ghost=true`
标记阶段，提前置 ghost 会污染同 tick 的 AOI/格扫读数。

---

## 3. `CM_SOFTCLOSE` 生命周期（第四条接线）

oracle 链（单线程）：

```
CM_SOFTCLOSE: if (!OffLineFlag) { BoReconnection=true; BoSoftClose=true; if (wParam==1) BoEmergencyClose=true; }
              ↳ 在 Run() 的消息循环里执行（:355-365），同一 Run() 的尾段（:367）立即 MakeGhost()
              ↳ !BoReconnection 假 ⇒ 不发 104
  → 下一轮 ProcessPlayObjectData 看到 Ghost ⇒ PlayObjectList.Remove; Disappear()
      → Party DelMember（:2139-2143）→ MyGuild.DelHumanObj(this)（:2144-2147）
          → CheckSaveGuildFile() → 清首个 Compare==1 槽
  → DealCancelA → SaveHumanRcd → CloseUser
```

（v3 写「RunNotice/ProcessPlayObjectData 走 BoSoftClose 支」不准确：`RunNotice` 只在 `!LoginNoticeOk` 时被调（`WorldServer.cs:650-653`），
而能发 `CM_SOFTCLOSE` 的玩家必然已 `LoginNoticeOk`（消息只在 `Run()` 里处理）；这里是**同一个 `Run()` 调用**的尾段。）

Rust：`handle_soft_close_cmd` 已做三道置位 + `world.soft_close()`（`main.rs:3530-3564`）。G3 在 `world.soft_close` **成功之后**调用
`guild_runtime::on_disconnect(..., bo_reconnection=true, bo_soft_close=true, ...)`：无 104/SS_208，做 check_save + 清槽（用会话的 `my_guild`）。

**只跑一次（双门）**：

1. `GuildSession.guild_done: bool`（显式 `bool`，`Default=false`；放在 `GuildSession` 而非 `SessionGround`，因为 `GuildSessions`
   trait 只暴露 `GuildSession`，模型用例用 `HashMap<String, GuildSession>`）。`on_disconnect` 进门即查、进门即置；
2. **无活会话 ⇒ 完全 no-op**：`sessions.guild(chr)` 为 `None` 直接返回。这一条是 C1 波纹带来的**必需**门：
   `drain_kicks`/`drain_soft_close` 会 `ground_sessions.remove`，之后同一角色 socket 关闭还会来一条 `Closed`；C1 下路径 A 会按名重新解析出会
   ⇒ 没有这条门就会**第二次发 104 并第二次清槽**。

两门都在 **(1) 覆盖之前** 判 ⇒ 第二次调用不会改写 `GuildSession.guild_name`、不会产出第二条 104。

- 重进世界会新建/覆盖 `SessionGround` ⇒ `guild_done=false`，不受影响（`materialize` 走 `..default()`）。
- W4-8a/8b 用成员 `[gm,a,b,bb,c]` 咬：第一次清 `b`，若第二次再清则 `bb` 被清（`Compare("bb","a")=1`），且会多一条 104。

---

## 4. 时钟域（v2 §2 收敛，仍有效）

- 唯一时钟：`world.clock_ms as i32`（未注入从 0 起、每 tick +200；`world-bevy/src/tick.rs:60`）。
- `guild_cmd::handle_guild_cmd` 的 `now_ms: i64` 改 `now_tick: i32`；调用点传 `world.clock_ms as i32`；`Ctx.now_tick` 直收；G2 侧只改唯一 rig `cmd()`（`guild_cmd.rs:892-911`）与签名（M-3）。
- `guild_runtime::startup` 调用点把 `guild_run_tick` 设为 `world.clock_ms as i32`（**加载时**，不是 0）。
- `step` 的门与所有落盘 `now` 只用传入 `now_tick`；`on_disconnect` 同。
- 源码锚点：`handle_guild_cmd(` 实参含 `world.clock_ms as i32`，且 `guild_cmd.rs` 无 `now_ms` 标记。
- 首跳：未注入（origin=0）⇒ 10200/20400 不触发 30s 保存，30600 首个 `CheckSaveGuildFile`；origin=3_600_000 ⇒ 3_610_200 首个。
  原因：加载时 `GuildConf.LoadConfig` 经 5 个 property setter 把 `BoChanged` 置真（`GuildInfo.cs:1090-1118`：`SetAuraePoint`/`SetBuildPoint`/
  `SetFlourishPoint`/`SetStabilityPoint`/`SetChiefItemCount`），`DwSaveTick` 保持 0（`GuildState::set_conf` 同）。
- 窗口用 `MIR2_CLOCK_MS=3600000`（>30000、正数）；禁用 epoch 量级（`as i32` 回绕）。
- C# 时钟注入仍未授权 ⇒ §10 B1-B4 `window_required`。

---

## 5. `Compare(...) == 1`（v2 §3 收敛，仍有效）

- .NET 8 实测：返回**码位差**（b/a=1、c/a=2、z/a=25、ab/a=1、a/ab=-1、B/a=1、丁/一=1、中/一=45、会乙/会甲=-9945、b/b=0）。
- 命中当且仅当：(A) 首个大小写折叠后不同的 UTF-16 码元差恰为 1；或 (B) `a` 以 `b` 为前缀（忽略大小写）且码元数恰多 1。
- 实现：`shared/hutil32.rs` 新增 `dotnet_compare_ordinal_ignore_case(a,b) -> i32`（`encode_utf16()` + `to_upper_char` 逐码元；前缀走长度差）。
  `guild.rs` 的 `del_human_obj_member` 用 `== 1`；**禁止** `cmp_ignore_case(...).cmp == Ordering::Greater`（现 `del_human_obj` 就是错的，
  `guild.rs:517-529`，本次一并删）。
- `cmp_ignore_case` 仍供 `GetRankName`（`world/guild.rs:429`）/`del_guild`（`:1001`）/`CancelGuld` 的「只判等」使用，不改语义；
  **`FindGuild` 不用它**——`find_guild` 是大小写敏感 `==`（`guild.rs:1013-1018`），不得改用 `cmp_ignore_case`（M-5 订正）。
- 空槽：`Compare("",name) = -len ≠ 1`，与 oracle `null` 的 -1 同判；`""`/`""` = 0。
- 已知分叉 `ı`/`ß`（`to_upper_char` vs .NET invariant）GBK 不可编码，按 §9 续号登记。

C7 矩阵（12 行）同 v2；变异：`==1→!=0/>0` ⇒ c/a、z/a、中/一 红；`==1→==0` ⇒ b/a 红；去长度分支 ⇒ ab/a、aa/a 红；`Ordering::Greater` ⇒ c/a 红。

---

## 6. 修正后的判据与变异点

通用：行为层只调生产接缝（`guild_runtime::{startup,step,on_disconnect}`、`handle_guild_cmd`），影子目录读文件字节；保鲜层用
`include_str!("main.rs")` 锚点（照既有 `drain_deal_tick_production_call_is_wired_in_sim_loop` 的写法：`main.rs:18094-18133`，整行 `trim()`
等值 + 实参逐行形状 + 作用域检查；`//`/`/*` 前缀会改变 trim 结果 ⇒ 注释行自动不命中）。`main.rs`/`guild.rs` 已核 `i/lf w/lf`。

### W1 `g3_startup_wiring` — 启动加载

**驱动**：`guild_runtime::startup(store: Option<&GuildStore>, sections: &GuildFileSections, rank_name_len: u8, now_tick: i32, server_index: i32) -> GuildManager`（`store=None` ⇒ 空表）。

**fixture**（影子目录，GBK）：

- `GuildList.txt` = `B\r\nA\r\nC\r\n`；
- `Guilds/B.txt`：自定义段头；`+N-B`；战表 `+B`；`#1 掌门X` + `+chiefB`；
- `Guilds/A.txt`：自定义段头；`+N-A`；战表 `+C`；`#1 掌门X` + `+chiefA`；
- `Guilds/C.txt` 缺失；
- `String.conf [Guild]`：`GuildNotice=公告X`、`GuildWar=敌对X`、`GuildAll=联盟X`、`GuildMember=成员段X`、`GuildMemberRank=职位X`、`GuildChief=掌门X`；
- `now_tick = 12_345`。

**断言**：

| # | 断言 | 牙 |
|---|---|---|
| 1 | 存活表 `names == [B,A]`，`guilds[0].name=="B"`、`guilds[1].name=="A"`（名单序，非字母序） | m8 加载后按名排序 ⇒ `[A,B]` 红 |
| 2 | `GuildList.txt` 回写为 GBK `B\r\nA\r\n`（含末 CRLF） | m7 末行不写 CRLF 红 |
| 3 | `B.ini`/`A.ini`/`C.ini` 都存在且 0 字节（C 的 `.ini` 在 `.txt` 缺失前已建） | m5 只给存活会建 `.ini` ⇒ C 缺 ⇒ 红 |
| 4 | `A.wars` 为空（引用 C，C 已逆序先移除）；`B.wars == [{guild_name:"B", war_tick:12345, war_time:0}]` | m4 `guild_exists` 用初始快照 ⇒ A 得战条 ⇒ 红 |
| 5 | `GuildList.txt` 缺失 ⇒ 空表、不 panic、不拒启（oracle `LoadGuildInfo` 的 `if (File.Exists)` 直接留空；Rust `MissingListFile` 可记 WRN，不作错误退出） | m6 当致命错误/拒启 ⇒ 红 |
| 6a | **段头解析**（4 键）：`A.notices==["N-A"]`、`A.ranks[0].members==["chiefA"]`、`B.wars` 非空 | m3 传 `from_config(&guild_castle_cfg)`（缺省头）⇒ 三者全空 ⇒ 红 |
| 6b | **配置**（拆两件，I-6/M-8）：① `GuildConfig`：`main.rs:14403` 用 `string_conf` 的 `[Guild]` **覆盖** `guild_chief=="掌门X"`、`guild_member_rank=="职位X"`（不覆盖就回落 `掌门人/行会成员`），该实例即送进 `handle_guild_cmd` 的那份；② `GuildMsgCfg`：`show_pre_fix_msg==true`、`prefix=="〖行会X〗"`、`f_color==0x0A`、`b_color==0x0B`，送进 `step`/`on_disconnect`。`GuildMsgCfg` 的颜色**取自 `GuildConfig.guild_msg_f_color/b_color`**（唯一来源，M-8），不另存一份。`sections` 四字段同 fixture。这两键是封号不是段头，行为断言在 W3-0/W3-1038b | W3-m8/m9；W4-2 |
| 7 | 存活会见表物化顺序与 `GuildList` 一致；`bo_changed==true`、`dw_save_tick==0`（`set_conf` 5 setter 置脏） | 若加载后不置脏 ⇒ W2 30600 不保存（W2-m7） |

**W1 变异**：m1 删/注释 `guild_runtime::startup(` 调用行 ⇒ 锚点红；m2 删 `store.load_guild_info` ⇒ 断言 1 红；m3 见 6a；m4 见 4；
m5 见 3；m6 见 5；m7 见 2；m8 加载后按名排序 ⇒ 断言 1 红。

### W2 `g3_tick_gate_and_30s_wiring` — 10s 门 / 战争到期 / 30s 门

**驱动**：`guild_runtime::step(guilds, store, sections, msg_cfg, server_index, now_tick, run_tick: &mut i32) -> StepOutcome`。

生产语义（`SystemProcessor.cs:70-77` + `GuildManager.cs:145-171`）：

```
if now_tick.wrapping_sub(*run_tick) > 10_000 {
    *run_tick = now_tick;
    for ev in guilds.run(now_tick) {          // 逐会：倒序扫战表 → WarEnded* → Update 保存 → CheckSave
        WarEnded{message}  ⇒ guild_broadcast_104(..., exclude: None)   // §2.2 同款管道
        Save{reason}       ⇒ save_one(..., server_index)
    }
}
```

`GuildSaveReason ∈ {UpdateGuildFile, CheckSaveGuildFile}`；`StepOutcome { ran, events, save_results }` 只用于可读，不改 oracle 语义。
`save_one` 返回 `Result<SaveStatus, GuildStoreError>`；**调用方**把 `MissingSaveTarget`/其它写错误一律吞掉并记
`保存行会信息失败!!! <path>`（`GuildInfo.cs:425-428`），不 panic、不重试（`save_one` 本身不吞，否则 W3-0 变体 2 不可断言）。

**用例 A（origin=0）**：加载后 `bo_changed=true, dw_save_tick=0, run_tick=0`；把内存 notices 改成 `["N2"]`（文件是 `N1`，使保存字节可见）。
`step` 依次 `10000/10200/20400/30600`：

- `10000`：`ran=false`（严格 `>`）；
- `10200`：`ran=true`，无 WarEnded、无 Save（`10200>30000` 假）；
- `20400`：无 Save；
- `30600`：`Save{CheckSaveGuildFile}`；文件出现 `+N2`；`bo_changed=false`；`dw_save_tick` 仍 0。
- 写侧段头：保存后的文件首行 `公告X`，且含整行 `敌对X`/`联盟X`/`成员段X`（4 键写侧；与 W1-6a 的读侧互补）。

**用例 B（origin=3_600_000）**：`run_tick=3_600_000, dw_save_tick=0`。首个 `step(3_610_200)`：门 `10_200>10_000` 真、`3_610_200>30_000` 真
⇒ `Save{CheckSaveGuildFile}`；`bo_changed=false`。`step(3_620_400)`：门真但 `bo_changed=false` ⇒ 无 Save。

**用例 C（战争到期 + 事件序）**：一会两战 `E1{war_tick:100, war_time:50}`、`E2{war_tick:1000, war_time:50}`，成员 `[b,c]` 在线。
**每步前显式** `*run_tick = now_tick - 10_001`：

- `now=150`：`*run_tick=-9851`；门过；`run` 判 `150-100=50>50` 假 ⇒ 无 WarEnded、无 Save；
- `now=151`：`*run_tick=-9850`；门过；`151-100=51>50` 真 ⇒ 事件序 = `WarEnded(E1)`（104 文本 `***E1行会战争结束`，前缀/颜色同 §2.2）**先**、
  `Save{UpdateGuildFile}` **后**；无 `CheckSaveGuildFile` Save（`dw_save_tick=151`）。文件里 E1 行消失、E2 行保留为 `+E2 899`
  （`50-(151-1000)`）。`bo_changed` 仍 true（`UpdateGuildFile` 置真，`CheckSave` 因 `151-151` 不触发，故不清）。

**W2 变异**：m1 删/注释 `guild_runtime::step(` 调用行与 `guild_run_tick` 赋值 ⇒ 锚点红；m2 `>`→`>=` ⇒ 用例 A 的 10000 `ran=true` 红；
m3 删 `guilds.run` ⇒ 用例 C 无 WarEnded、E1 留文件 红；m4 删 Save 应用 ⇒ 用例 A `+N2` 不出现 红；m5 调用点传 `now_ms()` ⇒ 锚点红 +
行为种子（`1_700_000_000_000i64 as i32`）使 30s 门永不触发 红；m6 事件序把 Save 排到 WarEnded 前 ⇒ 用例 C 红；m7 把 `bo_changed=false`
提前 ⇒ 用例 B 第二跳再 Save 红；m8 不扇出 WarEnded 104 ⇒ 用例 C 收件人空 红；m9 前缀/颜色/Series 写错 ⇒ 用例 C 帧断言红。

### W3 `g3_change_point_saves_wired` — 变更点立即落盘（含 W3-0 add_guild）

**rig**：`handle_guild_cmd`（G2 生产入口，`guild_cmd.rs:198`）扩参 `store/sections/msg_cfg/server_index`，`now_tick: i32`。G2 侧只需改**唯一**
rig `cmd()`（`guild_cmd.rs:892-911`）与函数签名——33 处 `.cmd(` 调用点不变（M-3 订正）。影子目录；
先 `startup` 后 `step(31_000)` 让 `bo_changed=false`（`31_000>30_000`，`run_tick=0`），再在 `now=40_000` 发命令。每个用例结束对比文件字节 + 状态。

| CM / 用例 | 断言（字节 + 状态） |
|---|---|
| 1038 成功 | `<名>.txt` 增加 `+target`（若目标为 rank99 首个，还增 `#99 职位X` 行）；`bo_changed=true`；`dw_save_tick==40000` |
| 1038b 无 rank99 的会 | 文件新增整行 `#99 职位X`（**GuildMemberRank 行为牙**） |
| 1039 删他人 | 文件少 `+target`；`bo_changed=true`；`dw_save_tick==40000` |
| 1039 自删解散（fixture：该会只有 rank1 + 掌门一人，命令由掌门本人发） | `<名>.txt` = 空会 7 行体（`公告X\r\n \r\n敌对X\r\n \r\n联盟X\r\n \r\n成员段X\r\n`，**文件仍存在**）；`GuildList.txt` 不含该名；`.bak` 与 `<GuildDir>/<名>/` 都不存在；`bo_changed`/`dw_save_tick` 不变 |
| 1040 公告 | 公告段逐字节更新；`bo_changed` 保持 false；`dw_save_tick` 保持 0（直写 `SaveGuildInfoFile`，不经 `UpdateGuildFile`） |
| 1041 成功 | ranks 已清（N3）⇒ 文件无 `#` 行；`bo_changed=true`；`dw_save_tick==40000` |
| 1044/1045 | 生产恒失败（769/-4、769/0）；无状态变化、无新写；文件字节不变 |
| **W3-0 add_guild** | **变体 1**：预存 `<新会>.txt`（旧体）⇒ 按 ctor 先 `load_guild_conf` 建 `.ini`，再 `add_guild` + `save_one`，之后文件 = 空会体 + `#1 掌门X` + `+chiefX`（**GuildChief 行为牙**）；`.ini` 0 字节；`GuildList.txt` 增该名；**新会** `bo_changed=false`、`dw_save_tick==0`（`SetGuildInfo` 不置脏；加载来的旧会仍是 `bo_changed=true`）。**变体 2**：`<新会>.txt` 缺失 ⇒ `save_one` 返回 `MissingSaveTarget`（吞掉）、`<新会>.txt` **仍不存在**、不 panic；`GuildList.txt` 仍增该名（`SaveGuildList` 在 `SetGuildInfo` 之后） |

**W3 变异**：m1 1038 删 `save_one` ⇒ 无 `+target` 红；m2 1039 删 `save_one` ⇒ 仍含 `+target` 红；m3 自删删 `save_list` ⇒ 名单仍含该名 红；
**m3b 自删删 `save_one`** ⇒ txt 仍是旧 `#1/+GM` 体 红；m4 1040 改用 `update_guild_file` ⇒ `bo_changed/dw_save_tick` 变 红；m5 任一臂用
`now_ms()`（epoch `as i32`）⇒ `dw_save_tick` 不等 红；m6 写 `.bak` 或建 `<名>/` 子目录 ⇒ 红；m7 保存侧改成「缺文件也创建」⇒ W3-0 变体 2
文件出现 红（**本条靠 W3-0 才可执行**）；m8 `add_guild` 用缺省 cfg ⇒ `#1 掌门人` 而非 `掌门X` 红；m9 1038 建 rank99 用缺省 ⇒ `#99 行会成员` 红。

> 写次数口径（不变）：自删解散 oracle 写空体两次（`CancelGuld` + `DelGuild` 各一次 `BackupGuildFile`），字节相同 ⇒ 不可观测；只断字节与名单。按 §9 续号登记。

### W4 `g3_disconnect_wiring` — 四条生命周期 + 104 + SS_208

**驱动**（C1 + C-1 + C-2 之后的形状）：

```rust
pub(crate) struct DisconnectCtx<'a> {
    pub chr_name: &'a str,
    pub bo_reconnection: bool,
    pub bo_soft_close: bool,
    pub now_tick: i32,        // world.clock_ms as i32
    pub server_index: i32,    // enter_cfg.server_index as i32
}

pub(crate) struct DisconnectOutcome {
    pub notices: Vec<(i32, Vec<u8>)>,            // 104：(收件人 actor_id, 已编码帧)，rank/member 序
    pub ss208: Option<ServerGroupMsgPayload>,    // C3
    pub saved: Option<Result<SaveStatus, GuildStoreError>>,   // check_save 命中时的落盘结果
    pub cleared: Option<String>,                 // 被清空槽的**旧成员名**
}

pub(crate) fn on_disconnect(
    guilds: &mut GuildManager,
    store: Option<&GuildStore>,
    sections: &GuildFileSections,
    msg_cfg: &GuildMsgCfg,
    sessions: &mut impl GuildSessions,           // 生产 = ground_sessions；模型用例 = HashMap<String, GuildSession>
    ctx: &DisconnectCtx<'_>,
    recipients: &dyn Fn(&str) -> Option<(i32, bool)>,   // 生产 = guild_recipients(..)；允许返回离场者
) -> DisconnectOutcome {
    let mut out = DisconnectOutcome::default();
    // 0) 双门（§3）：无会话条目 / 已做过 ⇒ 完全 no-op（在覆盖之前，防第二条 104）
    let Some(s) = sessions.guild(ctx.chr_name) else { return out };
    if s.guild_done { return out; }
    let path_a = !ctx.bo_reconnection && ctx.bo_soft_close;      // C-1：路径 A 只此一处定义
    let s = sessions.guild_mut(ctx.chr_name).expect("上面已确认存在");
    s.guild_done = true;
    // 1) C1 一个源：只有路径 A 覆盖并写回会话字段（oracle :382）；路径 B 用会话缓存
    let mut my_guild = s.guild_name.clone();
    if path_a {
        my_guild = guilds.member_of_guild(ctx.chr_name).map(str::to_string);   // 可为 None
        s.guild_name = my_guild.clone();
    }
    let Some(g) = my_guild.as_deref() else { return out };   // null ⇒ 无 104/无 SS_208/无存/无清
    // 2) C-1：104 + SS_208 **只在路径 A**（oracle 的 :380 门）
    if path_a {
        out.notices = guild_broadcast_104(
            guilds, g, &format!("{} 已经退出游戏.", ctx.chr_name), msg_cfg, recipients,
            Some(ctx.chr_name),                               // C2：显式排除离场者
        );
        out.ss208 = Some(ServerGroupMsgPayload {
            ident: messages::SS_208,
            server_index: ctx.server_index,
            payload: format!("{g}//{} has exited the game.", ctx.chr_name),
        });
    }
    // 3) 两条路径都做：先落盘（BoChanged=false，DwSaveTick 不动），再清槽
    if let Some(guild) = guilds.find_guild_mut(g) {
        if guild.check_save_guild_file(ctx.now_tick) {
            out.saved = Some(save_one(store, sections, guild, ctx.now_tick, ctx.server_index));
        }
        out.cleared = guild.del_human_obj_member(ctx.chr_name);   // Compare(...)==1 清首个
    }
    out
}
```

`guild_broadcast_104(guilds, guild_name, text, cfg, recipients, exclude)`（W2 的 WarEnded 104 也走它，`exclude=None`）：
按 `find_guild(guild_name)` → `cfg.show_pre_fix_msg` 拼前缀 → 逐 rank/逐 member：

```
if m.is_empty() { continue; }                          // MemberName 空 ⇒ PlayObject 为 null
if exclude == Some(m.as_str()) { continue; }           // C2：oracle 的 !Ghost 门（离场者）
let Some((aid, allow)) = recipients(m) else { continue };   // PlayObject == null / 实体缺失 / ghost
if !allow { continue; }                                // BanGuildChat == false
push((aid, sm_guild_message_104(aid, cfg.f_color, cfg.b_color, &text)))
```

**四条宿主接线（缺一红）**：

| # | 位置 | 调用点 | flags |
|---|---|---|---|
| 1 | `GateEvent::Closed`（`main.rs:15390` 臂）：`engine.delete_human(socket)`（15399）与 `kick_pending.retain`（15406）**之后**、`deal_cancel_between`（15409）之前；仍在 `online_take`（15420）与 `ground_sessions.remove(&chr_name)`（15438）之前 | `if !prepare_closed_guild(&mut ground_sessions, &online, &chr_name, index, socket) { ... continue; }` **整臂早退**（M-12/N-1）；随后 `let guild_closed = guild_runtime::on_disconnect(...)`；帧按 `SimToGate::SendFrame` 路由 | 置位在 `prepare_closed_guild` 内（早退之后）；实参 `bo_reconnection: sg.bo_reconnection,`（**不是**字面量 `false`） |
| 2 | `drain_kicks`（`main.rs:12108`）：`online_take`（12120）**之后**、`match world.disconnect(&chr)`（12139）之前、`ground_sessions.remove(&chr)`（12151）之前 | `let guild_kick = guild_runtime::on_disconnect(...)` | `bo_reconnection=false, bo_soft_close=false`（oracle 只置 `BoKickFlag`，`WorldServer.cs:1052-1063`） |
| 3 | `drain_soft_close`（`main.rs:12240`）：`online_take`（12252）**之后**、`world.disconnect`（12267）之前、`ground_sessions.remove(&chr)`（12276）之前 | `let guild_soft = guild_runtime::on_disconnect(...)`；置位在本函数内（M-10）：`if let Some(sg) = ground_sessions.get_mut(&chr) { sg.bo_soft_close = true; sg.bo_emergency_close = true; }` | `bo_reconnection=false, bo_soft_close=true`（照 `KickUser`，`TCPNetChannel.cs:268-269`） |
| 4 | `handle_soft_close_cmd`（`main.rs:3530`）：`world.soft_close` 成功后 | `let guild_softcmd = guild_runtime::on_disconnect(...)` | `bo_reconnection=true, bo_soft_close=true` |

生产收件人闭包（`main.rs`，与 `online_take` 同区）。**C-2**：`on_disconnect` 收 `&mut ground_sessions`，闭包不能再借同一张表，
所以先快照 `ban_guild_chat`：

```rust
// 每条宿主在调 on_disconnect 之前：
let guild_ban: HashMap<String, bool> = ground_sessions
    .iter()
    .map(|(k, sg)| (k.clone(), sg.guild.ban_guild_chat))
    .collect();
let guild_rcpt = guild_recipients(&online, &world, &guild_ban);

fn guild_recipients<'a>(online: &'a OnlineRegistry, world: &'a mir2_world::World,
                        ban: &'a HashMap<String, bool>)
    -> impl Fn(&str) -> Option<(i32, bool)> + 'a {
    move |name: &str| {
        let e = guild_cmd::find_play_object(online, world, name)?;   // 在线 + 实体存在 + !ghost
        let allow = ban.get(&e.chr_name).copied().unwrap_or(true);   // 缺条目按 oracle 缺省 true
        Some((e.actor_id, allow))
    }
}
```

快照必须在会话建立/更新之后、`&mut ground_sessions` 传入之前取；W4-11 用**同一份 `ban` 快照**构造 `guild_recipients`。
闭包不捕获 `ground_sessions`（否则 E0502）。

**`Closed` 臂的可测前置（N-1/N-3）**——把「陈旧早退 + 置位」抽成一个生产函数，供臂与用例共用（`main.rs`；用例用既有 `test_session_ground`（`main.rs:19684`）造会话）：

```rust
fn closed_event_is_current(online: &OnlineRegistry, chr_name: &str, gate_idx: i32, socket: i32) -> bool {
    // 比 (gate_idx, socket)：GateEvent::Closed 只带 index/socket，不带 game session_id
    online.iter().any(|e| e.chr_name == chr_name && e.gate_idx == gate_idx && e.socket == socket)
}

fn prepare_closed_guild(
    sessions: &mut HashMap<String, SessionGround>,
    online: &OnlineRegistry,
    chr_name: &str,
    gate_idx: i32,
    socket: i32,
) -> bool {
    if !closed_event_is_current(online, chr_name, gate_idx, socket) {
        return false;                       // 整臂早退（陈旧 Closed）
    }
    if let Some(sg) = sessions.get_mut(chr_name) {
        sg.bo_soft_close = true;            // CloseUser: if (!OffLineFlag)
    }
    true
}
```

臂内顺序（A5 钉住）：`engine.delete_human(socket)` → `kick_pending.retain(|n| n != &chr_name);`（15406）→
`if !prepare_closed_guild(&mut ground_sessions, &online, &chr_name, index, socket) { ... continue; }` → `deal_cancel_between(`（15409）→ …
**置位必须在早退之后**：否则陈旧 `Closed` 会把**新会话**的 `bo_soft_close` 置真，之后新会话走 `drain_kicks` 会从路径 B 变成路径 A 发 104
（这是 N-1 的实质）。`guild_closed` 的实参 `bo_reconnection` 必须取 `sg.bo_reconnection`（不能写死 `false`，否则 `CM_SOFTCLOSE` 之后
的 `Closed` 会误走路径 A）。

**断言**（fixture：会 `G` = rank1 `#1 掌门X`+`+gm`、rank99 `#99 职位X`+`+a +b +bb +c`；**默认 `sessions["a"].guild_name = Some("G")`**
（例外：6a = 会话条目存在但 `guild_name == None`；6d = 无会话条目；4b/6b/12 各用自带 fixture 并在行内明写）；文件 notices `N1`、内存 `N2`、
加载后 `bo_changed=true, dw_save_tick=0`；`msg_cfg`：`ShowPreFixMsg=1`、`GuildMsgPreFix=〖行会X〗`、`GuildMsgFColor=0x0A`、`GuildMsgBColor=0x0B`；
`server_index=0`，除非用例另写。**W4-4a/5/10b 都引用此默认会话**——否则路径 B 无会可清、m16/m22 无牙）：

| # | 断言 |
|---|---|
| W4-1 | `now=40000` 断 `a`（`bo_soft_close=true, bo_reconnection=false`，路径 A）：文件**仍有** `+gm/+a/+b/+bb/+c`、**无** `+\r\n`、出现 `+N2`（check_save 确实落盘）；`bo_changed=false`；`dw_save_tick` **仍 0**；`cleared==Some("b")`；内存 `b==""`、`count()==5`、`is_member("b")==false`、`is_member("")==false`、`bb`/`c` 不动 |
| W4-2a | `ShowPreFixMsg=1`（`GuildMsgPreFix=〖行会X〗`）：104 发到 `gm,b,bb,c`（**不发** `a`）；每帧 `Recog=收件人`、`Param=MakeWord(0x0A,0x0B)`、`Series=1`、尾 `EncodeString(GBK("〖行会X〗a 已经退出游戏."))`；发送序 = rank/member 序 |
| W4-2b | `ShowPreFixMsg=0`（**部署值**；`Server.conf:778=0`，评审实测）：尾 = `EncodeString(GBK("a 已经退出游戏."))`，**无前缀**（M-9） |
| W4-3 | 随后 `handle_guild_cmd(1040, "N3")`（由 `gm` 发，`now=41000`）⇒ 文件出现 `+\r\n`（b 的空槽）与 `+N3`；`bo_changed` 仍 false、`dw_save_tick` 仍 0 ⇒ **清槽在内存且由下一次直写带出**（不是「自动下一次保存」） |
| W4-4a | `bo_reconnection=true, bo_soft_close=true`（`CM_SOFTCLOSE` 形状）⇒ **无 104、`ss208==None`**，但 check_save+清槽各发生一次（正常 fixture） |
| W4-4b | **路径 B 不覆盖**（flags 钉死 `bo_reconnection=true, bo_soft_close=true`，I-5）：**manager `G` 的成员表 = `[b, c]`（不含 `a`）**，`session guild_name = Some("G")`（陈旧缓存）、`now=40000`、`bo_changed=true, dw_save_tick=0` ⇒ 无 104/`ss208==None`，但 `saved` 有值、`cleared==Some("b")` **发生在 manager 的 G 上**（若误按 manager 解析 ⇒ 全 no-op，m16 咬） |
| W4-5 | `bo_reconnection=false, bo_soft_close=false`（`drain_kicks` 形状）⇒ **无 104、`ss208==None`**；check_save + 清槽仍发生（正常 fixture） |
| W4-6a | **会话条目存在但 `guild_name == None`**（I-8 措辞）、manager 有会 + 路径 A ⇒ 104 照发（收件人 `gm,b,bb,c`）、`ss208==Some`、`saved` 有值、`cleared==Some("b")`；`sessions["a"].guild_name` 被写回 `Some("G")`（**C1 牙**：v3 模型会完全 no-op） |
| W4-6b | **会话 `Some("G")`、manager 无 `a`** + 路径 A（**manager `G` 的成员表 = `[b, c]`**，不含 `a`；`session guild_name = Some("G")`）⇒ 覆盖成 null ⇒ `notices` 空、`ss208==None`、`saved==None`、`cleared==None`；文件仍 `N1`（无 `+N2`）、`bo_changed` 仍 true；**manager 的 G 成员表 = `[b,c]` 原样**（M-4 措辞） |
| W4-6c | **manager 顺序钉死 `[H, G]`**（I-3）：两会同含 `a` 时 `member_of_guild` 返回**首个**命中（`GuildManager.cs:112-122`）⇒ 前置断言 `guilds.member_of_guild("a")==Some("H")`。fixture G=`+a +b`、H=`+a +hb`（`hb` 在线）、路径 A ⇒ 104 只发 `hb`、`ss208.payload` 以 `H//` 开头、`cleared==None`（H 里 `Compare("hb","a")≠1`）、G 的 `a/b` 原样、`sessions["a"].guild_name==Some("H")` |
| W4-6d | **无会话条目**（`sessions` 里根本没有该角色，I-8）、manager 有会 + 路径 A ⇒ 完全 no-op（`notices` 空、`ss208==None`、`saved==None`、`cleared==None`、G 不变） |
| W4-7 | 成员 `[a,c]` 断 `a` ⇒ `cleared==None`（`Compare("c","a")=2`）；成员 `[a]` 断 `a` ⇒ `None`（`Compare=0`）；成员 `[b,c]` 断 `c` ⇒ `None`（`-1`） |
| W4-8a | **路径 A 双调用**（I-4）：`bo_soft_close=true, bo_reconnection=false` 连续调两次 ⇒ 第二次 `notices` 空、`ss208==None`、`cleared==None`、`bb` 仍在、`bo_changed`/文件不变。第一次照 W4-1 |
| W4-8b | `CM_SOFTCLOSE`（一次，W4-4a 形状）后再来 `Closed`（路径 A）⇒ 第二次完全 no-op（`guild_done`）；`drain_soft_close` 后再来 `Closed` ⇒ `online` 已无该角色 ⇒ `closed_event_is_current == false` ⇒ **整臂早退**（W4-13 同门） |
| W4-9 | 收件人 `ban_guild_chat=false` 的成员不收款，但 `ss208` 仍 `Some`（`ban` 快照位=false） |
| W4-10a | `ss208` 三字段逐字：`ident==messages::SS_208(208)`、`server_index==7`（本轮 `EnterConfig.server_index=7`、`bo_changed=false` ⇒ `saved==None`）、`payload=="G//a has exited the game."` |
| W4-10b | **flags 钉死**（I-5）：① `bo_reconnection=true, bo_soft_close=true`；② `bo_reconnection=false, bo_soft_close=false` ⇒ 两例都 `notices` 空、`ss208==None` |
| W4-10c | 路径 A + manager 无会（同 W4-6b）⇒ `ss208==None` |
| W4-10d | 会里只有离场者（成员 `[a]`）⇒ `notices` 为空 **但** `ss208==Some`（门是 `MyGuild != null`，不是「帧发得出去」） |
| W4-11 | **C2**：`online` **仍含 `a`**（实体存在、`ghost=false`，即 `Closed` 的真实时序）+ 生产形状 `guild_recipients`（**同一份 `ban` 快照**，C-2）⇒ `notices` 里**没有** `a` 的 `actor_id`；其余四条照旧。**注**：用例不得先把 `a` 从 `online` 摘掉，否则删掉排除行也不会红（牙失效） |
| W4-12 | **解散后另一在线成员断线（I-2 订正）**：fixture：manager 的 `G` = 单 rank、成员 `[a, hb]`（**两人都经 `AddMember` 绑定**）；删除走 **GM `del_guild("G")`**（`GuildManager.cs:34-52` 只拒 `RankList.Count > 1`，单 rank 多成员可删；**不是** 1039 自删——`CancelGuld`（`GuildInfo.cs:680-697`、`world/guild.rs:501-513`）要求 `RankList.Count==1 && MemberList.Count==1`，两成员时必 false，G 不会被移除）。删后 manager 无 G、`hb` 会话仍 `Some("G")`。随后 `hb` 断线：路径 A（`bo_reconnection=false, bo_soft_close=true`）⇒ `notices` 空、`ss208==None`、`saved==None`、`cleared==None`（`member_of_guild("hb")==None`）；路径 B（`bo_reconnection=false, bo_soft_close=false`，`drain_kicks` 形状）⇒ 同上（`find_guild("G")==None`）。oracle：`DelGuild→BackupGuildFile`（`:602-608`）把**已绑定**的 `hb` 清成 `MyGuild=null` ⇒ `Disappear` 全空；若 `hb` 是 **unbound**（从文件加载），oracle 不清其 `MyGuild`，断线对**游离对象**做 check_save+清槽（重写空体，字节等价；T-96 同族） |
| W4-13 | **陈旧 `Closed` 与重登（M-12，整臂早退）——helper 级**：`a` 触发 104 后（`guild_done=true`）重登（`ground_sessions.insert`，`main.rs:16072`，新 socket 501），旧 socket 500 的 `Closed` 到达 ⇒ 整臂早退。**纯函数 `closed_event_is_current(online, chr, gate_idx, socket)`** 四例：`(a,0,501)==true`、`(a,0,500)==false`、`(a,1,501)==false`（跨网关）、`(不存在,0,500)==false`。臂级行为由 **W4-14** + 锚点 A5 钉住 |
| W4-14 | **陈旧 `Closed` 后新会话 flags 不变（arm-level，N-1/N-3）**：用既有 `test_session_ground`（`main.rs:19684`）建 `sessions["a"]`，置 `bo_soft_close=false, bo_reconnection=true`（模拟此前 `CM_SOFTCLOSE`）；`online` 的 a = `(gate_idx,socket)=(0,501)`。① 陈旧调用 `prepare_closed_guild(&mut sessions, &online, "a", 0, 500)` ⇒ 返回 **false**、`bo_soft_close` **仍 false**（置位没发生在早退之前）、`bo_reconnection` 仍 true；② 当前调用 `(…, 0, 501)` ⇒ 返回 **true**、`bo_soft_close == true`、`bo_reconnection` **仍 true**（臂不得写死 `false`）；③ ② 之后调 `on_disconnect(bo_reconnection=true, bo_soft_close=true)` ⇒ 路径 B：无 104/SS_208、check_save+清槽（= `CM_SOFTCLOSE` 后再 `Closed` 的模型级复现）。牙：置位在早退之前 ⇒ ① 红；`bo_reconnection` 写死 false ⇒ ② 红且 ③ 误发 104 |
| 锚点 A1 | `main.rs` 四条调用行（trim 后逐字、各唯一，且落在各自作用域内）：`let guild_closed = guild_runtime::on_disconnect(`、`let guild_kick = guild_runtime::on_disconnect(`、`let guild_soft = guild_runtime::on_disconnect(`、`let guild_softcmd = guild_runtime::on_disconnect(`；`guild_closed` 在 `GateEvent::Closed {` 臂内、`deal_cancel_between(`（15409）与 `online_take`（15420）之前；`guild_kick`/`guild_soft` 在各自 `online_take`（12120/12252）之后、`world.disconnect`（12139/12267）之前；**三条路径（closed/kick/soft）必须在对应 `ground_sessions.remove`（15438/12151/12276）之前**；`guild_softcmd` 走 `handle_soft_close_cmd`，**没有** `ground_sessions.remove`，只要求 `fn handle_soft_close_cmd(` 体内且 `world.soft_close(chr_name)` 之后（M-2 拆条） |
| 锚点 A2 | `guild_runtime.rs` 的 `guild_broadcast_104` 体内两行逐字：`if exclude == Some(m.as_str()) {` + `continue;`（**C2 的排除行**） |
| 锚点 A3 | `main.rs`：`fn guild_recipients<'a>(` 定义行唯一；签名含 `ban: &'a HashMap<String, bool>`；体内 5 行内出现 `guild_cmd::find_play_object(online, world, name)?` 且**不出现** `ground_sessions`；四处 `let guild_rcpt = guild_recipients(&online, &world, &guild_ban);` 各唯一（C-2） |
| 锚点 A4 | 生产接线（I-7）：① `guild_runtime::startup(` 唯一，实参含 `&guild_store`/`&guild_sections`；② `let mut guild_run_tick = world.clock_ms as i32;` 唯一（加载时初值）；③ `guild_runtime::step(` 唯一，实参含 `&mut guild_run_tick,`；④ `guild_cmd::handle_guild_cmd(` 调用点（`main.rs:15337`）末参行为 `world.clock_ms as i32,`，且**该调用点**不再出现 `now_ms(),`（当前 `main.rs:15352` 是 `now_ms(),`；`main.rs` 其它 `now_ms()` 调用点 16137/16154/16159/16588/16859/16868 等**不受限**） |
| 锚点 A5（N-7，v7 修订） | ① `fn prepare_closed_guild(` 内 `if !closed_event_is_current(&online, &chr_name, gate_idx, socket) {` 唯一，其后 3 行内含 `return false;`；`sg.bo_soft_close = true;` **在该早退之后**（`gate_line < set_line`）、`return true;` 之前。② `fn drain_soft_close(` 内 `sg.bo_soft_close = true;` 与 `sg.bo_emergency_close = true;` 各 1 处，且在 `guild_soft` 之前。③ `Closed` 臂 `if !prepare_closed_guild(&mut ground_sessions, &online, &chr_name, index, socket) {` 唯一，位于 `engine.delete_human(socket)` 与 `kick_pending.retain(|n| n != &chr_name);`（15406）**之后**、`deal_cancel_between(`（15409）之前，其后 3 行内含 `continue;`。④ `guild_closed` 实参含 `bo_reconnection: sg.bo_reconnection,`（**不是** `bo_reconnection: false,`）。牙：删/挪任一置位 ⇒ 生产走路径 B / 不发 104 / 陈旧 Close 污染新会话（模型级仍绿，A5 + W4-14 咬）；删早退 ⇒ A5③ + W4-14 ① 红 |

**W4 变异**：m1 删 `Closed` 的 `on_disconnect` ⇒ 锚点 + W4-1 红；m2 把会话值取在 `ground_sessions.remove` 之后（路径 B 无会可清）⇒ W4-4b/W4-5 红
（路径 A 仍绿——**这正是 C1 之后 m2 的旧期望作废之处**）；m3 先清后 check_save ⇒ W4-1 文件出现 `+\r\n` 红；m4 不调 check_save / 忽略返回值 ⇒
`+N2` 不出现且 `bo_changed` 仍 true 红；m5 删 `drain_kicks` 调用 ⇒ 锚点红（行为无 104 不红，锚点是唯一牙）；**m5b 删 `handle_soft_close_cmd`/
`drain_soft_close` 调用 ⇒ 对应锚点红 + W4-4a/W4-8a 红**；m6 用 `Ordering::Greater` ⇒ W4-7 的 `[a,c]` 红（c 被清）；
m7 104 在清后 ⇒ `b` 不收 ⇒ W4-2 红；m8 104 恒发 ⇒ W4-4a/W4-5 红；m9 条件恒假 ⇒ W4-2 红；m10 忽略 `BanGuildChat` ⇒ W4-9 红；
m11 去掉 `guild_done` ⇒ W4-8a 红；m12 不拼前缀/前缀空 ⇒ W4-2 尾红；m13 颜色互换或 `Series` 非 1 ⇒ W4-2 红；
**m14 不做 `member_of_guild` 覆盖（104/清槽全用会话）⇒ W4-6a 红**；**m15 覆盖只喂 104、清槽另读会话 ⇒ W4-6b 红（G 被清 + 104 发进 G）**；
**m16 覆盖条件写成 `if ctx.bo_soft_close`（漏 `!bo_reconnection`）⇒ W4-4b 红**；**m17 删锚点 A2 的排除行 ⇒ W4-11 红**；
**m18 `guild_done` 门挪到 104 之后 ⇒ W4-8a 红（第二次调用再发一条 104）**；**m19 去掉「无会话条目 ⇒ no-op」门（改用 manager 兜底）⇒ W4-6d/W4-8b 红**；
**m20 `ss208` 用 `ident=0` / 漏 `server_index` / 少一个 `/` ⇒ W4-10a 红**；**m21 `ss208` 只在 `notices` 非空时产 ⇒ W4-10d 红**；
**m22 把 104/SS_208 挪出 `if path_a`（C-1 的原始缺陷）⇒ W4-10b 的 ②（`false/false`）红**；
**m23 把 `prepare_closed_guild` 的早退改成恒 `return true`（或删掉 `closed_event_is_current` 调用）⇒ W4-14 ① 红 + A5③ 红**；
纯 helper 四例（W4-13）仍绿——它们只测 `closed_event_is_current` 本身，这正是 v6 的牙失效点（N-3）。

### 其余判据去向（同 v2）

| 原编号 | 去向 |
|---|---|
| T2 文件字节 | C2；落盘统一 `guild_runtime::save_one`，主循环/变更点各一条锚点 |
| T3 10s 严格 `>` | W2-A/C |
| T5 战争到期事件序 | W2-C |
| T7 Compare | §5 C7 |
| T9 绑定非在线 / 104 | C9 + W2-C（WarEnded 104）+ W4（退出 104） |
| T10 104 线形 | C10；`Recog=ActorId`、`Param=MakeWord(F,B)`、`Series=1`、尾 GBK |
| T11 非 0 区 | 可选；W4-10a 只用它断 `server_index` 字段（`bo_changed=false`，不触落盘） |

---

## 7. 写集与串行顺序

| # | 文件 | 改动 |
|---|---|---|
| 1 | `mir2-rs/crates/shared/src/hutil32.rs` | `dotnet_compare_ordinal_ignore_case`（+ 单测矩阵；复用 `to_upper_char`） |
| 2 | `mir2-rs/crates/world/src/guild.rs` | `del_human_obj_member(name) -> Option<String>`（纯清，Compare==1，返回被清旧名）/ 删旧 `del_human_obj`；`run -> Vec<GuildRunEvent>`（WarEnded + Save{reason}，保留逐会倒序与 `Update→CheckSave` 次序）；`GuildSaveReason`；`to_record`/`names`/`del_guild -> Option<GuildRecord>`；`ref_member_name` 保持；改 3 条既有单测（`:1512`、`:1529`、`:1926`）与 `run` 两条（M-3 订正） |
| 3 | `mir2-rs/crates/data/src/guild.rs` | `GuildFileSections::from_strings(notice: &str, war: &str, all: &str, member: &str)`（4 段头；**不能**收 `bins` 的 `GuildStrings`，避免 data→bins 反向依赖；保留 `from_config` 但 G3 不用） |
| 4 | `mir2-rs/bins/game-svr/src/guild_runtime.rs`（新建） | `startup`/`step`/`on_disconnect`/`save_one`/`save_list`/`guild_broadcast_104`；`DisconnectCtx`/`DisconnectOutcome`/`ServerGroupMsgPayload`；`GuildMsgCfg`（`show_pre_fix_msg`/`prefix` + 颜色**转发**自 `GuildConfig`，M-8）；只做接线 |
| 5 | `mir2-rs/bins/game-svr/src/guild_cmd.rs` | `now_tick: i32`；`Ctx` 增 `store/sections/msg_cfg/server_index`；1038/1039/1040/1041 落盘；`GuildSession` 增 `guild_done: bool`（`Default=false`，`materialize` 走 `..default()` ⇒ 重登复位）；只改**唯一** rig `cmd()`（`:892-911`）与签名，33 处 `.cmd(` 调用不变（M-3） |
| 6 | `mir2-rs/bins/game-svr/src/main.rs` | `EnterConfig` 增 `guild_store/guild_sections/guild_msg_cfg`（注释 `from_config`→`from_strings`，构造点 `main.rs:904`/`12614`）；**`main.rs:14403` 用 `string_conf [Guild]` 覆盖 `guild_chief/guild_member_rank`**（I-6）；`sim_loop` 加载 + `guild_run_tick = world.clock_ms as i32` + `step`；`guild_recipients` + **`ban` 快照**（C-2）；**四条**断线 + 104 + `guild_done` + `closed_event_is_current`/`prepare_closed_guild` 与 `Closed` **整臂早退**（M-12/N-1）；`handle_guild_cmd` 末参改 `world.clock_ms as i32`（现 `:15352` 是 `now_ms()`）；锚点用例 A1/A3/A4 |
| 7 | `mir2-rs/bins/game-svr/src/strings.rs` | `[Guild]` 六键 + `[String] GuildMsgPreFix`（缺省 `〖行会〗`）；section 解析由单 bool 改 `Option<Section>` |
| 8 | `mir2-rs/bins/game-svr/src/config.rs` | `[Setup] GuildMsgFColor/GuildMsgBColor`（缺省 `0xDB/0xFF`）→ 写进 `GuildConfig.guild_msg_f_color/b_color`（**唯一来源**，M-8）；`ShowPreFixMsg` 已有（`config.rs:301-304/1102`），接到 `GuildMsgCfg.show_pre_fix_msg` |
| 9 | `mir2-rs/tests/parity/whitelist.md` | **T-96..T-102**（§9；其余条目按 §9-0 续号） |

不碰：`src/**`、`sql/**`、`MirClient/`、`crates/protocol`、`crates/storage`、`E:/MirServer`、`/e/tmp/wup`、`tools/`。

串行：S1 shared+world → S2 data → S3 strings/config → S4 guild_runtime → S5 guild_cmd → S6 main → S7 whitelist/门禁。理由同 v2
（API 基础先行；`main.rs` 最后动，压一次 rebase）。

---

## 8. 更新后的 G3 DoD

- [ ] S1-S7 完成；`git status` 干净；`git diff --stat <基线> -- src/ sql/ MirClient/ crates/protocol crates/storage` 为空；
- [ ] 四门禁全绿（用本车道 `CARGO_TARGET_DIR`）：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
      `cargo test --workspace`、`cargo run -p msgcodegen -- --check`；
- [ ] W1：`B,A,C` 顺序牙；4 段头**读+写**；`GuildChief/GuildMemberRank` 配置断言（6b）；
- [ ] W2：用例 A/B/C；C 每步 `*run_tick = now-10001`；写侧 4 段头；WarEnded 104；
- [ ] W3：1038/1038b/1039/1039 自删/1040/1041/1044/1045；**W3-0 add_guild 两变体**（`#1 掌门X`、缺文件不建）；自删空体 7 行；
- [ ] W4：四处宿主接线；**C1 单源三后果（6a/6b/6c）+ 无会话条目门（6d）**；104 条件表（发/不发四路）；104 早于清槽；
      `CheckSave` 不推 `DwSaveTick`；断线后文件仍是旧体、`+\r\n` 由后续 1040 带出；`guild_done` 单次清理且**第二条 104 为零**；
- [ ] **C-1**：`path_a` 单变量；104/SS_208 只在路径 A（W4-10b 两例 + m22）；
- [ ] **C-2**：`ban` 快照，闭包不捕获 `ground_sessions`（`cargo clippy -D warnings` 下无 E0502；锚点 A3）；
- [ ] **I-2/T-97**：W4-12（**GM `del_guild`** 解散后另一 **bound** 成员断线，路径 A/B 都 no-op；unbound 情形按 T-96 观察）；
- [ ] **M-12/T-101**：W4-13 驱动 `closed_event_is_current` 四例；**W4-14** 驱动 `prepare_closed_guild`（陈旧不置位 / 当前保留 `bo_reconnection`）；
      `Closed` 整臂早退且早退在 `kick_pending.retain` 之后；锚点 A5 四条；T-101 按 `tier=R` + `window_required: true` 收口；
- [ ] **I-1/T-96**：夹具假设写进 W2-C/W4 头注；窗口 A 记录实际收件人集；
- [ ] **C2**：W4-11 用「`a` 仍在 `online`」的生产形状；锚点 A1/A2/A3 全绿；
- [ ] **C3**：W4-10a/10b/10c/10d 逐字段 + 逐条件；事件三段分开存（不拼死串）；
- [ ] C7 12 行全绿（含 c/a、中/一、ab/a、会乙/会甲）；
- [ ] 变异日志：§6 每条「注入 → 指定用例红 → 还原（sha256）」有记录；**W3-m7 必须由 W3-0 变体 2 咬红**；**W4-m14..m22 全部可执行**；
- [ ] §10 构造级全绿；窗口 A 由 owner 执行并留证据；
- [ ] §10 B1-B4 未授权 ⇒ `review` 写 `window_required:true` + 缺项清单；`tier=R` + `tier_why` 点名
      `SaveGuildInfoFile`/`CheckSaveGuildFile`/`DelHumanObj`/10s+30s 门/104 时序；
- [ ] **T-96..T-102** 已写 `whitelist.md`，§9 其余条目按 §9-0 续号；
- [ ] 收尾：删本车道 `CARGO_TARGET_DIR`；文档线 worktree 用于本文件，实现线 worktree 按 Cleanup DoD 清理；无进程、无端口、无 `E:/MirServer` 改动。

---

## 9. 待写入 `whitelist.md` 的条目（当前最大 T-95，**尚未写入**）

> **9-0（编号）**：以下条目**尚未写入** `whitelist.md`；当前最大 **T-95**（2026-10-20，master `35c039c8d`）。本批按序续号：
> **T-96** 收件人缓存绑定（§9-18）、**T-97** 解散会话簿记（§9-19）、**T-98** pre-login（§9-15）、**T-99** 挂机超时（§9-16）、
> **T-100** 账号到期（§9-17）、**T-101** 陈旧 `Closed`（§9-20）、**T-102** 104 发送口径（§9-21）；§9 的 1–14 条在合并日按当时最大号顺延
> （不得复用 T-96..T-102）。**若合并日 T-96..T-102 已被占用，按当时最大号顺延，不复用**。不得写 `T-9x` 字面量。

1. **节拍量化（M-6 订正）**：world tick 200ms vs oracle 50ms 扫描。每次 10s 跳晚 **+150ms**（10200/20400/30600 vs 10050/20100/30150），
   n 跳累计 **150n ms**；30s 门在**第三次**跳触发（30600 vs 30150 ⇒ 晚 **450ms**）。战争到期**相对 oracle 第 n 跳晚 150n ms**（绝对到期延迟两侧均 ≤10s 量级）；命令读上一 tick 的
   `world.clock_ms`（≤200ms）。
2. **未注入首存 30600**；窗口强制 `MIR2_CLOCK_MS=3600000`。
3. **顶号同 tick 清理**：oracle `MakeGhost` → 下一次 `ProcessHumans`（≤50ms）`DelHumanObj`；Rust 同 tick；仅相位差。
4. **保存编码**：Rust 恒 GBK；oracle 按 BOM 嗅探；部署件无 BOM。
5. **加载失败降级**：`.ini` 创建失败 oracle 抛到 `InitializeWorld` catch；Rust 记 ERR、空表。
6. **绑定是加载期快照**：`actor_id` 可能重登复用；极端情况收件人不同。
7. **`to_upper_char` 与 .NET 的 `ı`/`ß` 分叉**（GBK 不可编码）。
8. **自删解散写空体两次**（字节相同，写次数不可观测）。
9. **104 与 `DelHumanObj` 的相位差（v4 订正）**：oracle 两者都在**同一线程**、同一 `ProcessPlayObjectData` 的不同轮次
   （104 在 `Run()` `:697`，移除+`Disappear` 在下一轮的 `:707-716`；`UserProcessor.cs:27-36` 单线程 50ms）——**不是线程竞态**。
   Rust 压进同一次 `on_disconnect`，序 104→check_save→清；差别只有 ≤50ms 相位与「下一轮才清」的可见窗口。
10. **`SS_208` 无接收端**：单服只落事件/日志；线上字节 = `"{ident}/{server_index}/{payload}"` 的 GBK。同一 `SS_208` 还承载行会聊天转发
    （`PlayObject.Chat.cs:172/177`，`payload = "<会>/<名>/<文本>"`），G3 只做退出支。
11. **`CheckSaveGuildFile` 的存盘通常是重复写**：`UpdateGuildFile` 已即时落盘；30s 后的 check_save 只重写同字节。`DelHumanObj` 的清槽在其自身
    check **之后**且不置脏，故该次 check 也写不出清槽。故用 `N2`（内存）vs `N1`（文件）的种子使保存字节可见；**生产可达的 `bo_changed=true`
    来自加载时 `GuildConf.LoadConfig` 的 5 个 setter**。
12. **`.bak` 路径**：oracle `BackupGuildFile` 写 `<GuildDir>/<名>/.<tick>.bak`，父目录不存在 ⇒ `FileMode.Open` 抛错被吞；Rust 不建模写 `.bak`。
13. **`GuildCastleConfig.guild_chief/guild_member_rank` 不是生产源**：oracle 从 String.conf `[Guild]` 读（`StringConf.cs:30-35`）；G3 必须在
    `main.rs:14403` 构造后**覆盖**（§6 W1-6b/W3-m8/m9），否则静默回落 `掌门人/行会成员`。
14. （跟踪缺口，非白名单）**`GuildOfficial.ReQuestBuildGuild` 未移植**：W3-0 只锁 `add_guild` + `save_one/save_list` 组合（生产函数），
    不声称 NPC 调用点已接线；另开卡 `openmir2-m3-guild-create-npc`。
15. **T-98 pre-login 断线不发 104**：`LoginNoticeOk == false` 时 `ProcessPlayObjectData` 调 `RunNotice()`（`WorldServer.cs:648-653`），
    后者只有 `MakeGhost()`、**没有 104 块**（`PlayObject.cs:1079-1091`）⇒ oracle 不发 104/SS_208，但仍 `Disappear` ⇒ check_save+清槽。
    本仓未建模 `LoginNoticeOk`（只有 `inbound::client_login_notice_ok` 的截断逻辑）⇒ `Closed` 臂会对「进世界但未确认公告」的连接**多发 104**。
    可复现：客户端进世界后不发 `CM_LOGINNOTICEOK(1018)` 直接断开。**建议**（不强制扩 G3）：`SessionGround` 增 `login_notice_ok: bool`，
    `Closed` 调 `on_disconnect` 时若 false 则只做 check_save+清槽、不产 104/SS_208（与路径 B 同形）。`window_required: false`（构造级可判）。
16. **T-99 挂机超时软关**：`ActionOfOffLine`（`ExecutionProcessingSys.cs:2716-2731`）置 `OffLineFlag=true` + `KickOffLineTick`；到点后
    `PlayObject.Message.cs:235-238` 置 `OffLineFlag=false; BoSoftClose=true` ⇒ 本表「路径 A」⇒ **会发 104**（`BoReconnection` 为 false 时）。
    本仓 `OffLineFlag`/`KickOffLineTick` 与脚本动作 `offline` 均未建模 ⇒ 无此路径。另两处同源未建模：① `CloseUser` 的 `if (!OffLineFlag)`
    （`ChannelMessageHandler.cs:391`）恒按 `false` 处理；② **收件人**侧 `SendSocket` 的 `OffLineFlag` 门（`PlayObject.Socket.cs:107-110`）
    会让挂机成员收不到任何 104（本仓不建模 ⇒ 挂机成员会「多收」帧）。
17. **T-100 账号到期硬关**：`PayMentMode == 3` 且 `AccountExpired` ⇒ `BoEmergencyClose=true`（仅；`PlayObject.Message.cs:37-52`）⇒ MakeGhost，
    但 `BoSoftClose=false` ⇒ **无 104**；清槽照旧（`Disappear`）。本仓未解析 `PayMentMode`（`ServerConf.cs:22`）/`AccountExpired` ⇒ 无此路径。
18. **T-96：104/行会消息的收件人集 = 缓存 `GuildMember.PlayObject`（oracle 缓存 bug）**。`SendGuildMsg`（`GuildInfo.cs:444-448`）读的是
    `RankList` 里缓存的 `PlayObject`，`null` 跳过。**有效绑定点只有三处**：`LoadGuild`（`:333-338`；**启动时** `GetPlayObject` 为空 ⇒ null，**重载时**在线成员会被重新绑定）、
    `SetGuildInfo`（`:474-478`）、`AddMember`（`:645-649`）。**`:774-778` 不是绑定点**：它是 `UpdateRank`（`:708` 起）的解析循环，临时表随后被
    `RankList = rankListTmp`（`:966-971`，**空表**）丢弃（未变更路径 `:817-822` 也 `ClearRankList`）。登录重绑**被注释**（`:495`、`:980`）
    ⇒ 重启后从文件加载的成员收不到任何行会消息（104/WarEnded/聊天），直到被重新 `AddMember`。GM **`ReloadAllGuild`**
    （`ReloadAllGuildCommand.cs:21` → `GuildMgr.LoadGuildInfo()`，`GuildManager.cs:78`）走 `LoadGuild` ⇒ **重载会对当前在线成员重新绑定**。
    本仓以 `online` 解析（等价于修掉该缓存 bug；也是 G2 `SendGuildMsg` 未落地的口径）。**夹具假设**：W2-C/W4 的收件人断言只覆盖本进程内
    `add_guild`/`AddMember`（或 GM 重载）进入的成员；**窗口 A** 用 live 建会/加人并记录实际收件人集（§10.2）。
    `tier=R`；`tier_why`：收件人可达性取决于进程内对象缓存，构造级只能覆盖 bound 夹具，必须真机观察。
19. **T-97：GM `DelGuild`/`ReloadGuild` 后已绑定（bound）成员的会话未清（G2 缺口）**。oracle `BackupGuildFile`
    （`GuildInfo.cs:591-621`）对每个**已绑定**成员 `MyGuild=null; RefRankInfo(0,"")`（`:602-608`，`playObject != null` 才清）；本仓的
    1039 自删只 `clear_guild(self.chr_name)`（`guild_cmd.rs:621`；该路径只可能发生在**单成员会**，不产生「其它成员」）；对本条有意义的是
    GM `del_guild` 路径（若接线）只动 manager、不动任何会话。**两种成员的结果**：
    ① **bound**（本进程 `AddMember`/建会/重载绑定）⇒ oracle 清会话，本仓不清 ⇒ `SM_SENDUSERSTATE`/queryuser 的会名残留至断线；
    ② **unbound**（从文件加载，T-96）⇒ oracle **也不清**（`:602-608` 的 `playObject != null` 跳过），其 `MyGuild` 悬空，断线时对游离对象做
    check_save+清槽（重写空体，字节等价）。**不实现「清所有在线」**的理由：会与 unbound 成员的真实行为形成反向偏差；断线路径两条都因
    `find_guild == None` 无副作用（W4-12）。`tier=R`（与 T-96 统一）；`tier_why`：会话会名经查询帧对外可见，且 bound/unbound 取决于进程内
    缓存状态，构造级只能覆盖 bound 夹具，真机须纳入 §10.2 的观察项。
20. **T-101 陈旧 `Closed`（M-12）——整臂早退**：`ground_sessions.insert`（`main.rs:16072`）在重登时覆盖会话（`guild_done=false`）。
    `Closed` 臂在 `engine.delete_human(socket)`（15399）与 `kick_pending.retain`（15406）**之后**、`deal_cancel_between`（15409）之前调
    `prepare_closed_guild`；helper `closed_event_is_current(online, chr, gate_idx, socket)` 比 `(gate_idx, socket)`（`GateEvent::Closed` 只带
    `index`/`chr_name`/`socket`，不带 game `session_id`）。不匹配即 `continue`，跳过 `on_disconnect`、`online_take`（15420）、
    `saves.remove`（15436）、`ground_sessions.remove`（15438）、party、`world.disconnect`（15472）、`deal_cancel_between`（15409）。
    **残余**：① `engine.delete_human(socket)` 仍执行（socket 键，与 oracle `CloseUser` 按 socket 查 `UserList` 同口径）；② **同网关 socket 号复用**：
    若网关在同一 `gate_idx` 上复用 socket 号，旧 `Closed` 仍可能误匹配新会话 ⇒ 需给事件补 generation 或窗口构造「快速重连复用 socket」；
    ③ `KickAccount`/`drain_kicks`/`drain_soft_close` 仍按 chr/account 清理（既有窗口，非 G3 新增）；④ `kick_pending.retain` **已挪到早退之前**
    （v6 曾把它放在早退之后，会让陈旧 Close 留下 pending kick，进而由 `drain_kicks` 踢掉新会话——v7 消除该项）。
    `tier=R`（按 tier 政策含「时序/竞态」）；`tier_why`：早退是陈旧事件 vs 重登的竞态面，构造级只覆盖「号不复用」的主路径；②需窗口或夹具
    构造 ⇒ `window_required: true`。

21. **T-102 104 的发送口径（M-11）**：宿主立即 `SimToGate::SendFrame`（与 party/NPC 帧同口径）；oracle 是 `SendMsg` 入 `MsgQueue`、
    下一次 `ProcessBufferSend` 才写 socket。同一广播内 rank/member 序一致；跨帧类型**且跨收件人**的相对序不可观测；同一收件人同时看到
    104 与 party/交易帧时的固定序见 §1-11。`tier=L`；`tier_why`：发送路径差异（入队 vs 立即）本身不触 RNG/持久化/数值；**边界**：§1-11 的
    「104 先于 party/交易帧」只在构造级证明到「宿主按该序发出」这一层（调用位置 + 锚点 A1），客户端可见的跨帧交错（网关/网络聚合）未证明
    ⇒ 若 §10.2 的 104 真机项未观察到该序，按 **R** 处理。

---

## 10. C# 时钟注入授权边界（同 v2）

- §10.1 构造级（无需 C# 授权）：W1/W2/W3/W4 行为 + C2/C7/C9/C10 + 锚点 + 变异注入。
- §10.2 真机窗口 A（不需注入）：C# 开机 >30s；Rust `MIR2_CLOCK_MS=3600000`；两会影子目录。断言首个 10s Run 后两会 `<名>.txt` 重写、
  战表行消失、公告/成员段逐字节不变、`GuildList.txt` 不变、`.ini` 0 字节、无 `.bak`；掉线用「差 1/差 2」两组观察下一份直写里的 `+\r\n`。
  **104/SS_208 真机项（I-9）**：两个在线行会成员（**本会话内 live 建会/加人**，见 T-96），一方断线 ⇒ 另一方**恰收一条** 104
  （`Recog`=收件人 ActorId、`Param=MakeWord(F,B)`、`Series=1`、尾 = `EncodeString(GBK(<名> 已经退出游戏.))`；部署 `ShowPreFixMsg=0` ⇒
  **无前缀**），离场方**零帧**；`SS_208` **恰一行**且 `payload = "<会名>//<名> has exited the game."`；同时记录 C# 侧实际收件人集
  （T-96 的窗口观测）。
- §10.3 窗口 B（须 C# 时钟注入，blocked）：B1 `dw_save_tick` 绝对值；B2 10s/30s 精确边界；B3 i32 回绕；B4 战争到期逐毫秒对拍。
  未授权 ⇒ `not_verified` / `window_required:true`。
- §10.4 review 措辞：`tier: R`；只做构造级 ⇒ `approve + window_required:true`；A 完成 B 未完成 ⇒ 仍 `window_required:true`；A+B 全绿才可裸 `approve`。

---

## 11. 复核命令（只读）

```bash
git rev-parse HEAD                                             # 35c039c8d
sed -n '367,389p'   src/M2Server/Player/PlayObject.Message.cs   # MakeGhost + MyGuild 覆盖(:382) + 104 + SS_208（在 Run() 尾段）
sed -n '2129,2150p' src/M2Server/Player/PlayObject.Message.cs   # Disappear: party → MyGuild.DelHumanObj
sed -n '1720,1722p' src/M2Server/Player/PlayObject.Message.cs   # RM_GUILDMESSAGE → SM_GUILDMESSAGE 帧形
sed -n '874,884p'   src/M2Server/Player/PlayObject.Message.cs   # CM_SOFTCLOSE 三道置位
sed -n '235,238p'   src/M2Server/Player/PlayObject.Message.cs   # 挂机超时置 BoSoftClose（§9-16）
sed -n '37,52p'     src/M2Server/Player/PlayObject.Message.cs   # 账号到期只置 BoEmergencyClose（§9-17）
sed -n '1079,1091p' src/M2Server/Player/PlayObject.cs           # RunNotice：MakeGhost 无 104 块（§9-15）
sed -n '645,720p'   src/GameSrv/Word/WorldServer.cs             # ProcessPlayObjectData：Run(:697) / Ghost → Disappear(:707-716)
sed -n '885,895p'   src/GameSrv/Word/WorldServer.cs             # SendServerGroupMsg 拼 "nCode/nIdx/sMsg"
sed -n '388,394p'   src/M2Server/Net/ChannelMessageHandler.cs    # CloseUser 置 BoSoftClose（:391 是 if (!OffLineFlag)）
sed -n '261,270p'   src/M2Server/Net/TCP/TCPNetChannel.cs       # KickUser 置 Emergency/SoftClose
sed -n '333,338p;444,451p;474,478p;645,649p;774,778p' src/M2Server/Guild/GuildInfo.cs  # PlayObject 绑定四点 + 104 读点（T-96）
sed -n '485,505p'   src/M2Server/Guild/GuildInfo.cs             # GetRankName：PlayObject 重绑被注释(:495, :980)
sed -n '591,621p'   src/M2Server/Guild/GuildInfo.cs             # BackupGuildFile：解散清 MyGuild(:602-608)（I-2）
sed -n '431,456p'   src/M2Server/Guild/GuildInfo.cs             # SendGuildMsg 前缀/颜色/BanGuildChat
sed -n '525,550p'   src/M2Server/Guild/GuildInfo.cs             # CheckSave / DelHumanObj(Compare==1)
sed -n '1090,1118p' src/M2Server/Guild/GuildInfo.cs             # 5 个 setter 置 BoChanged
sed -n '145,171p'   src/M2Server/Guild/GuildManager.cs          # Run 次序
sed -n '19,32p;66,76p;112,122p' src/M2Server/Guild/GuildManager.cs  # AddGuild / FindGuild(敏感) / MemberOfGuild(顺序)
sed -n '70,78p'     src/GameSrv/Word/Threads/SystemProcessor.cs # 10s 门
sed -n '27,36p'     src/GameSrv/Word/Threads/UserProcessor.cs   # 单线程 50ms ProcessHumans（§9-9）
sed -n '112,171p'   src/GameSrv/Npc/GuildOfficial.cs            # ReQuestBuildGuild（未移植）
sed -n '30,38p'     src/Modules/SystemModule/Conf/StringConf.cs # [Guild] 六键 + [String] GuildMsgPreFix
sed -n '2716,2731p' src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs  # offLine 脚本动作（§9-16）
sed -n '18094,18133p' mir2-rs/bins/game-svr/src/main.rs         # 锚点既有写法（drain_deal_tick…）
awk 'NR>=12105&&NR<=12155||NR>=12240&&NR<=12280' mir2-rs/bins/game-svr/src/main.rs  # drain_kicks/soft_close 调用序（M-1）
awk 'NR>=15390&&NR<=15440' mir2-rs/bins/game-svr/src/main.rs   # Closed 臂：deal_cancel(15409)/online_take(15420)/remove(15438)
sed -n '14400,14405p;15345,15353p;16070,16074p' mir2-rs/bins/game-svr/src/main.rs # guild_cfg 构造 / now_ms() 调用 / 会话 insert
sed -n '198p;606,636p;892,911p' mir2-rs/bins/game-svr/src/guild_cmd.rs  # 入口 / 自删只清自己 / 唯一 rig（M-3）
sed -n '429p;1001,1018p;1512p;1529p;1926p' mir2-rs/crates/world/src/guild.rs  # cmp_ignore_case 用点 / find_guild / 待改单测
```

---

## 12. 残留物

- 本文件（v7）+ v6/v5/v4/v3（各自顶部加一行「已被 v7 / v6 / v5 / v4 取代」的指引，正文未改）+ v2/首版（背景）。
- 文档 worktree `/e/tmp/g3v4`（分支 `g3v4`）：只含本文件与四份取代指引；**不含**代码改动。
- 本机临时物：`E:/tmp/g3v4-pass*.py`、`E:/tmp/g3v4-v6-pass*.py`、`E:/tmp/g3v4-v7-pass*.py`（改稿脚本，交付后删除）。
- `E:/tmp/laneB-cmp-net8/` 探针由前序车道保管（只读）。
- 无 `CARGO_TARGET_DIR`、无编译产物、无进程、无端口、无 `E:/MirServer` 改动。

---

## v7 变更记录（2026-10-20）

> 第三轮复核：12 条清单全部 Closed；本表只列本轮 N-1..N-5。

| 编号 | 改法 | 位置（v7） |
|---|---|---|
| **N-1**（Important） | 把「陈旧早退 + 置位」抽成 `prepare_closed_guild`；A5 加顺序断言「`sg.bo_soft_close = true;` 在 `if !closed_event_is_current(...)` 的 `return false;` **之后**、`return true;` 之前」；新增 A5④「`bo_reconnection: sg.bo_reconnection,`（非字面量 `false`）」；新增 **W4-14** arm-level：陈旧 Close 后新会话 `bo_soft_close` 仍 false、`bo_reconnection` 保持原值；当前 Close 置 `bo_soft_close=true` 且保留 `bo_reconnection`；随后 `on_disconnect` 走路径 B | §6 helper、宿主表、A5、W4-14 |
| **N-2**（Important） | (a) helper 改比 `(gate_idx, socket)`；`GateEvent::Closed` 不带 game `session_id` ⇒「同网关 socket 号复用」登记为 T-101 残余②。(b) 早退挪到 `kick_pending.retain`（15406）**之后**、`deal_cancel_between`（15409）之前，消除 v6 新增的「陈旧 Close 不清理 pending kick ⇒ 后续 `drain_kicks` 踢新会话」行为（残余④写明）。(c) T-101 改 `tier=R` + `tier_why` + `window_required: true` | A5③、宿主表、§9-20 |
| **N-3**（Minor，无牙） | m23 改为「把 `prepare_closed_guild` 的早退改成恒 `return true` ⇒ **W4-14 ① + A5③** 红」，并注明纯 helper 四例（W4-13）此时仍绿 | m23 |
| **N-4**（Minor） | T-96 改「**启动时** `GetPlayObject` 为空 ⇒ null；**重载时**在线成员会被重新绑定」，消除与 `ReloadAllGuild` 句的矛盾 | §9-18 |
| **N-5**（可选） | T-102 `tier_why` 补边界：构造级只证到「宿主按序发出」；客户端可见的跨帧交错未证明 ⇒ 真机未观察到则按 R | §9-21 |

---

## v6 变更记录（2026-10-20，历史保留）

> 第二轮复审：C-1/I-3/I-4/I-6/I-8/I-9 已 Closed；本表只列本轮 12 条。

| 清单 | 改法 | 位置（v6） |
|---|---|---|
| 1（必改） | §2.7 的收件人闭包仍写 `&ground_sessions`（与 C-2 矛盾、照抄 E0502）⇒ 改为 `guild_recipients(&online, &world, &guild_ban)` | §2.7 |
| 2（必改） | **W4-12 前置不成立**：`CancelGuld`（`GuildInfo.cs:680-697`、`world/guild.rs:501-513`）要求 `RankList.Count==1 && MemberList.Count==1` ⇒ 两成员 fixture 必 false。改走 **GM `del_guild`**（`GuildManager.cs:34-52` 只拒 `RankList.Count>1`）或合成态；fixture 明写 `hb` 经 `AddMember` **绑定**。**T-97 改述**：GM `DelGuild`/`ReloadGuild` 后 **bound** 成员会话未清；**unbound** 成员 oracle 仍悬空（T-96 同族）；明写 `:602-608` 只清 bound。删掉「1039 自删后其它在线成员」 | W4-12、§9-19 |
| 3（必改） | **W4-13/m23 不可执行**（`on_disconnect` 无 socket）⇒ 选**整臂早退**：新增纯 helper `closed_event_is_current(online, chr, socket)`；`Closed` 臂在 `engine.delete_human` 后、`kick_pending.retain`/`deal_cancel_between` 前 `continue`。W4-13 驱动 helper 三例；T-101 残余缩为 `engine.delete_human`（socket 键）+ drains 的既有窗口 | W4-13、A5、§9-20、m23 |
| 4（必改） | A4④ 的「不再出现 `now_ms(),`」限定到 `handle_guild_cmd(` 调用点末参（`main.rs` 另有 16137/16154/16159/16588/16859/16868 等）；A1 拆「三条路径早于 `ground_sessions.remove`（15438/12151/12276）+ `guild_softcmd` 无 remove」 | A1/A4 |
| 5（必改） | 新增锚点 **A5（N-7）**：`Closed` 的 `sg.bo_soft_close = true;`、`drain_soft_close` 的 `sg.bo_soft_close/bo_emergency_close`、`if !closed_event_is_current(...) {` 早退行（按作用域断言） | A5 |
| 6（必改） | W4 fixture 头写明默认 `sessions["a"].guild_name = Some("G")`（例外 6a/6d/4b/6b/12）；W4-4a/5/10b 引用 | W4 断言头 |
| 7（Minor） | T-96：`:774-778` 改述为 `UpdateRank` 解析循环（`RankList = rankListTmp`（`:966-971`）丢弃）；有效绑定点 = LoadGuild/SetGuildInfo/AddMember；补 GM `ReloadAllGuild`（`ReloadAllGuildCommand.cs:21`）重绑 | §2.1a、§9-18 |
| 8（Minor） | §9-1 战争到期改「相对 oracle 第 n 跳晚 150n ms；绝对到期延迟两侧均 ≤10s 量级」 | §9-1 |
| 9（Minor） | §9 标题改「待写入 `whitelist.md` 的条目（当前最大 T-95，尚未写入）」；9-0 加「若 T-96..T-102 被占用，按当时最大号顺延，不复用」 | §9、§9-0 |
| 10（Minor） | §2.1a 表改「会话条目存在但 `guild_name == None`」；W4-4b/6b/12 去掉「G = +a +b」幻影成员表，统一「manager G = [...]；session guild_name = Some("G")」 | §2.1a、W4 |
| 11（Minor） | §2.5 `drain_soft_close` 行加「若无既有 `BoReconnection`」限定 | §2.5 |
| 12（Minor） | T-97 与 T-96 统一 `tier=R` + `tier_why`；T-101/T-102 标 `tier=L` + `tier_why` | §9 |

---

## v5 变更记录（2026-10-20，历史保留）

### C-1（Critical）— 104/SS_208 只在路径 A

- **问题**：v4 伪代码（v4:443-459）把 104/SS_208 放在 `if !bo_reconnection && bo_soft_close` 之外 ⇒ 路径 B 会照发。
- **改法**：`let path_a = !ctx.bo_reconnection && ctx.bo_soft_close;` 只定义一次；覆盖写、`out.notices`、`out.ss208` 全包在 `if path_a` 内；
  check_save/清槽**不分路径**（§6 W4 伪代码）。
- **牙**：m22 = 把 104/SS_208 挪出 `if path_a` ⇒ W4-10b 的 ②（`false/false`）红（W4-4b/10b 的 flags 已按 I-5 钉死）。

### C-2（Critical）— 闭包借用冲突（E0502）

- **问题**：v4 的生产闭包捕获 `&ground_sessions`，与 `on_disconnect(..., &mut ground_sessions, ...)` 同时借用同一张表。
- **改法**：§2.7 + §6 W4：调用前快照 `HashMap<String, bool>`（`ban_guild_chat`），`guild_recipients(online, world, &ban)` 只捕获
  `&online`/`&world`/`&ban`；W4-11 用同一份快照；锚点 A3 断言签名含 `ban: &'a HashMap<String, bool>` 且体内不出现 `ground_sessions`。

### Important

| # | 改法 | 位置 |
|---|---|---|
| I-1 | 收件人 = 缓存 `GuildMember.PlayObject`（`GuildInfo.cs:444-448`；绑定 `:333-338/:474-478/:645-649/:774-778`；重绑被注释 `:495`/`:980`）；登记 **T-96** + 夹具假设 + 窗口 A 记录实际收件人集 | §2.1a、§9-18、§10.2 |
| I-2 | 撤回 §9-18「游离对象」：`BackupGuildFile`（`GuildInfo.cs:591-621`）对已绑定成员 `MyGuild=null`（`:602-608`）；Rust 自删只清自己（`guild_cmd.rs:621`）⇒ 登记 **T-97**；新增 W4-12（解散后另一在线成员断线，路径 A/B 都 no-op） | §9-19、W4-12 |
| I-3 | W4-6c 钉 manager 顺序 `[H, G]` + 前置断言 `member_of_guild("a")==Some("H")`（`GuildManager.cs:112-122`） | W4-6c |
| I-4 | W4-8a 改**路径 A 双调用**（`true/false` 连续两次）；CM_SOFTCLOSE 归 W4-4a/8b；m18 改为「门挪到 104 之后 ⇒ 第二次再发一条」 | W4-8a、m18 |
| I-5 | W4-4b 钉 `bo_reconnection=true, bo_soft_close=true`；W4-10b 钉 `true/true` 与 `false/false` 两例（m16/m22 才有牙） | W4-4b/10b |
| I-6 | W1-6b 拆两件：`GuildConfig`（`main.rs:14403` 用 `[Guild]` 覆盖 `guild_chief/guild_member_rank`，送 `handle_guild_cmd`）与 `GuildMsgCfg`（送 `step`/`on_disconnect`）；写集补 14403 与 strings/config 落点 | W1-6b、§7 |
| I-7 | 新增锚点 A4：`startup` / `guild_run_tick = world.clock_ms as i32` / `step(..., &mut guild_run_tick, ...)` / `handle_guild_cmd` 末参 `world.clock_ms as i32`（现 `:15352` 为 `now_ms()`）；A1 补「四条都在对应 `ground_sessions.remove` 之前」 | 锚点 A1/A4 |
| I-8 | W4-6a = 「会话条目存在但 `guild_name == None`」；W4-6d = 「无会话条目」 | W4-6a/6d |
| I-9 | 窗口 A 增 104/SS_208 真机项（两成员、一方断线；对端恰一条 104、离场方零帧、`SS_208` 恰一行、`ShowPreFixMsg=0` 无前缀） | §10.2 |

### Minor

| # | 处理 |
|---|---|
| M-1 | **采纳**：只有 `Closed` 在 `online_take` 之前；`drain_kicks`/`drain_soft_close` 在 `online_take`（12120/12252）之后、`world.disconnect`（12139/12267）之前。§2.7 与宿主表已改，并说明显式排除对 drains 冗余。 |
| M-2 | **采纳**：锚点 A1 补「四条都必须在对应 `ground_sessions.remove`（15438/12151/12276）之前」。 |
| M-3 | **采纳**：写集单测行号改 `:1512`/`:1529`/`:1926`；「13 条 G2 测试」改为唯一 rig `cmd()`（`guild_cmd.rs:892-911`）+ 33 处 `.cmd(` 不变。 |
| M-4 | **采纳**：W4-6b 措辞改为「manager 的 G 成员表 = `[b,c]` 原样」。 |
| M-5 | **采纳**：§5 从 `cmp_ignore_case` 列表移除 `FindGuild`；注明 `find_guild` 是 `==`（`guild.rs:1013-1018`），`cmp_ignore_case` 用点是 `get_rank_name:429`/`del_guild:1001`。 |
| M-6 | **采纳**：§9-1 改为每次跳 +150ms、n 跳 150n；30s 门第三次跳 30600 vs 30150（晚 450ms）。 |
| M-7 | **采纳**：`GuildManager.cs` 112-122 / 66-76 / 19-32；`GuildInfo.cs` 1090-1118；`ChannelMessageHandler.cs:391`。 |
| M-8 | **采纳**：颜色唯一来源 = `GuildConfig.guild_msg_f_color/b_color`；`GuildMsgCfg` 只转发（`show_pre_fix_msg`/`prefix` + 颜色）。 |
| M-9 | **采纳**：新增 W4-2b，`ShowPreFixMsg=0` 时尾无前缀（部署值）。 |
| M-10 | **采纳**：`handle_kick_account` 保持无 `sessions` 参数；在 `drain_soft_close` 内、调 `on_disconnect` 前对 `ground_sessions.get_mut(&chr)` 置 `bo_soft_close/bo_emergency_close`。 |
| M-11 | **采纳（登记）**：§2.2 + §9-21 写明「立即 SendFrame vs 入队」口径与可观测边界，不新增判据。 |
| M-12 | **采纳（实现门）**：`Closed` 臂加 `entry.socket == event.socket` 校验；W4-13 + m23；残余登记 §9-20。 |
| M-13 | **采纳**：§1-11 改为「104 是帧，与 party/交易帧的相对序可观测」，并给出 oracle 三段序。 |
| M-14 | **采纳**：W4-8a/8b 的第二次调用语义与 §9-20 的 socket 校验联动（W4-13 单列）。 |

---

## v4 变更记录（2026-10-20，历史保留）

### C1（模型错误，重要）— 104 与清槽是**一个**源

- **oracle 依据**：`PlayObject.Message.cs:382`（`MyGuild = MemberOfGuild(ChrName)` 覆盖会话字段，可 null）→ `:383/:385/:386`（104/SS_208）→
  下一轮 `:707-716` 移除 + `Disappear()` → `:2144-2147`（`if (MyGuild != null) MyGuild.DelHumanObj(this)`）；
  `DelHumanObj` 只有这一个调用点（`GuildInfo.cs:534-550`，`:542` 的 `Compare==1`）。
- **改法**：§2.1a 新增「一个源」：路径 A 覆盖并写回 `GuildSession.guild_name`，104/`ss208`/check_save/清槽全用同一个 `my_guild`；
  路径 B 不覆盖、用会话值。删除 v3「104 按名查 / 清槽按会话」的两源写法（§6 W4 伪代码重写）。
- **改后的期望**：W4-6a（会话 None + manager 有会 ⇒ 发 104 + 落盘 + 清 `b`）、W4-6b（会话 Some(G) + manager 无会 ⇒ 全 no-op，G 不动）、
  W4-6c（会话 G、manager H ⇒ 全在 H 上发生，G 不动）；W4-4b 补「路径 B 用会话值」。
- **m2 重定义**：v3 的「`guild_name` 取在 `ground_sessions.remove` 之后 ⇒ W4-1/2 红」在 C1 下**失去牙**（路径 A 改按名解析）。
  现 m2 = 「会话值取在 remove 之后」⇒ **W4-4b / W4-5 红**（路径 B 唯一依赖会话）。
- **新牙**：m14（不做覆盖）⇒ W4-6a 红；m15（覆盖只喂 104）⇒ W4-6b 红；m16（漏 `!bo_reconnection`）⇒ W4-4b 红。
- **波纹**：`guild_done` 从 `SessionGround` 移到 `GuildSession`（trait 可及）+ 新增「无活会话 ⇒ no-op」门；
  W4-6d/8b（drain 之后到来的 `Closed` 必须完全 no-op），m19 咬。

### C2（接线缺口，重要）— 显式排除离场者

- **oracle 依据**：`MakeGhost()`（`PlayObject.Message.cs:375`）早于 `SendGuildMsg`（`:385`）；`SendMsg` 的 `!Ghost` 门
  （`BaseObject.Message.cs:144`）在**收件人**上判。
- **改法**：§2.7 新增：`guild_broadcast_104(..., exclude: Option<&str>)`，`on_disconnect` 传 `Some(ctx.chr_name)`；W2 的 WarEnded 传 `None`。
  生产 `recipients` 闭包另行走 `guild_cmd::find_play_object`（在线 + 实体存在 + `!ghost`），两层都要。
- **为什么不能只靠闭包**：`Closed`/`drain_kicks`/`drain_soft_close` 都在 `online_take`/`world.disconnect` **之前**调 `on_disconnect`
  ⇒ 离场者仍在 `online` 且 `ghost=false` ⇒ 闭包会返回它。
- **用例/锚点**：W4-11（`online` **仍含** `a`，生产形状闭包 ⇒ 无 `a` 的帧）；锚点 A2（`guild_runtime.rs` 的
  `if exclude == Some(m.as_str()) {` + `continue;`）、A3（`fn guild_recipients<'a>(` + `find_play_object` + 四处调用行）、
  A1（四处 `let guild_{closed,kick,soft,softcmd} = guild_runtime::on_disconnect(` 及作用域/先后）。
- **牙**：m17 删排除行 ⇒ W4-11 红（**前提**：用例不先摘 `online`）。

### C3（覆盖缺口）— `SS_208` 逐字段断言

- **oracle 依据**：`PlayObject.Message.cs:386`；拼装在 `WorldServer.cs:885-895`（`nCode + "/" + nServerIdx + "/" + sMsg`）；
  `SS_208 = 208`（`src/OpenMir2/Messages.cs:457`、`mir2-rs/crates/protocol/src/messages.rs:598`）。
- **改法**：§2.4 事件拆三段 `ServerGroupMsgPayload { ident, server_index, payload }`（不拼死串），`describe()` 固定形；
  触发门 = 路径 A 且 `my_guild == Some`，与 104 帧数无关。
- **断言**：W4-10a（`ident==208`、`server_index==7`、`payload=="G//a has exited the game."`）、W4-10b（路径 B 两例 ⇒ None）、
  W4-10c（路径 A + manager 无会 ⇒ None）、W4-10d（会里只剩离场者 ⇒ 0 帧但 `ss208==Some`）。
- **牙**：m20（ident/`server_index`/分隔符）、m21（只在有帧时产）、m22（路径 B 也产）。

### 附：v4 复核中发现的其它不一致（v3 正文与 oracle 不符、评审未列）

1. **方法名**：104 块在 `PlayObject.Run()`（`Message.cs:20-399`，块 `:367-389`），由 `WorldServer.ProcessPlayObjectData()` 调 `Run()`
   （`WorldServer.cs:697`）。v3 写「`ProcessPlayObjectData` 尾段」——层级对、方法名错。
2. **行号**：`Disappear` 起 `:2129`，guild 段 `:2144-2147`（v3/会话层文档写 `:2138-2142` 是 party 段）。
3. **§9-9 定性错误**：104 与 `DelHumanObj` 同线程、同一次 `ProcessHumans` 的两轮（`UserProcessor.cs:27-36`）⇒ 不是线程竞态。
4. **条件表漏 pre-login**：`LoginNoticeOk == false` 走 `RunNotice`（无 104 块）⇒ 不发 104、但仍清槽（`WorldServer.cs:648-653`、`PlayObject.cs:1079-1091`）⇒ §9-15。
5. **条件表漏挂机超时**：`OffLineFlag` 超时置 `BoSoftClose` ⇒ **会发 104**（`PlayObject.Message.cs:235-238`；脚本置位 `ExecutionProcessingSys.cs:2716-2731`）⇒ §9-16。
6. **条件表漏账号到期**：只置 `BoEmergencyClose` ⇒ MakeGhost 但无 104（`PlayObject.Message.cs:37-52`）⇒ §9-17。
7. **§3 的 CM_SOFTCLOSE 链路**：不是 `RunNotice/ProcessPlayObjectData` 支，而是**同一个 `Run()`** 的消息循环之后立即到 `:367`。
8. **`SS_208` 线上前缀**：`nCode/nServerIdx/sMsg` 三段拼装（`WorldServer.cs:889`）v3 未提，是 C3 字段拆分的依据。
9. **空尾门行号**：v3 引的 `PlayObject.Socket.cs:108-111` 实际是 `OffLineFlag` 门；空文本门在 `:111-114`（见 §2.2/§9-16）。
10. **陈旧行号/引用**：`main.rs` 四处断线（`15390/15409/15420`、`12108`、`12240`、`3530`）、`guild_cmd.rs` 入口 `:198`、
   `main.rs:12609` 的 `server_index`、锚点范例 `main.rs:18094-18133`、`GuildInfo.cs` 5 setter `:1088-1117`。
11. **T-9x 编号**：`whitelist.md` 当前最大 T-95；`T-9x` 必须落成具体号（§9-0）。
