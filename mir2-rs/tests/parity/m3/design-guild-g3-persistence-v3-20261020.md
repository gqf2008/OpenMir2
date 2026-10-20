# Lane B — 行会 G3（持久化 + 10s/30s + 掉线）实施简报 v3
> **已被 v4 取代**：`design-guild-g3-persistence-v4-20261020.md`（C1/C2/C3 已并入正文），本文仅作历史留档，判据以 v4 为准。

- 基线：`ae669377f`（master，2026-10-20）。
- 车道：Lane B **只读**（不建 worktree、不改仓库、不起栈、不碰 `E:/MirServer`、不进 `/e/tmp/wup` 与 `tools/` 判读脚本车道）。
- 取代 `mir2-rs/tests/parity/m3/design-guild-g3-persistence-v2-20261020.md`（v2 及其评审更正）。
- v3 只改判据与接线口径；v2 中未被点名的章节（时钟域、Compare 语义、窗口 A/B 授权边界）继续有效，已在本文收敛重述。
- 只读取证：`src/**`、`mir2-rs/**`（均未修改；`git status` 干净）。

---

## 摘要（≤800 字）

v3 修 2 处 parity 漏洞 + 6 处假绿。① **断线 104**：`ProcessPlayObjectData` 在 `MakeGhost()` 后按
`!BoReconnection && BoSoftClose`，用 `MemberOfGuild(ChrName)` 向会内在线成员发 `<名> 已经退出游戏.`
（104：Recog=收件人、Param=MakeWord(F,B)、Series=1、GBK 尾、ShowPreFixMsg 前缀），SS_208 单服无接收端；
`Closed`/账号顶号发，`BoKickFlag` 与 `CM_SOFTCLOSE` 不发；扇出须早于 `DelHumanObj`。
② **`CM_SOFTCLOSE`** 是第四条生命周期（置位→MakeGhost→Disappear→CheckSave→DelHumanObj），须入 W4，
并用 `SessionGround.guild_done` 防二次清理。③ W2-C 每步显式 `run_tick=now-10001`。
④ W1 名单改 `B,A,C`。⑤ 六键=4 段头+2 封号：W1 断段头与配置，`#1 掌门X`/`#99 职位X` 由 W3 的
add_guild/1038 行为断。⑥ W3 补 add_guild 用例使 m7 可执行，并补「自删解散漏 save_one」变异。
⑦ W4 删掉「下一次保存写出 `+`」：`CheckSave` 只清 `BoChanged`、不推 `DwSaveTick`，清槽不置脏
⇒ 断线后文件仍旧且不会自动再存；`+\r\n` 由后续 1040 带出。DoD 相应更新。

---

## 0. v3 变更总览（评审 8 点逐条落点）

| # | 评审点 | v3 落点 | 牙（哪条变异必红） |
|---|---|---|---|
| 1 | 断线 104 退出提示被漏 | §2 规格；W4-2/4/5 | W4-m9/m12/m13；104 编码器既有单测 |
| 2 | `CM_SOFTCLOSE` 生命路径未入清单 | §3；W4-4/W4-8；`guild_done` | W4-m5b/m11 |
| 3 | W2 用例 C 与 10s 门矛盾 | W2-C：每步 `*run_tick = now - 10001` | W2-m2（`>`→`>=` 另有用例 A） |
| 4 | W1 顺序牙与字母序重合 | W1 fixture 改 `B,A,C` | W1-m8（加载后按名排序） |
| 5 | W1 只测 4 个段头 | §6 W1-6b + W3-0 + W3-1038b | W3-m8（Chief 用缺省）/ m9（MemberRank 用缺省） |
| 6 | W3 缺 add_guild 用例、m7 不可执行 | W3-0；W3-m7 | W3-m7（保存改为缺文件也创建） |
| 7 | W3 缺「自删解散漏 save_one」变异 | W3-m3b | 文件仍含旧 `#1/+GM` |
| 8 | W4「下一次保存写出 `+\r\n`」是错的 | §6 W4 断言 1/2/3 + 说明 | W4-m3（清在存前）/ m4（不存） |
| 附 | （v3 自查新增）同一会话二次清理 | `guild_done` | W4-m11 |

---

## 1. 对 v2 的关键订正（先读）

| # | v2 | v3 订正 |
|---|---|---|
| 1-1 | W4 只列 `Closed`/`drain_kicks`/`drain_soft_close` 三处，断线只做 check_save+清成员 | **四处**：加 `CM_SOFTCLOSE`；且 `Closed`/账号顶号还要发 **104 退出提示**（§2）。三/四处都必须"只跑一次"（`guild_done`）。 |
| 1-2 | W4 断言「下一次保存写出 `+\r\n`」 | **错**。`CheckSaveGuildFile`（`GuildInfo.cs:525-533`）只置 `BoChanged=false` + 落盘，**不更新 `DwSaveTick`**；`DelHumanObj`（`:534-550`）清槽**不置 `BoChanged`**。⇒ 断线后文件仍是旧体（含 `+b`），清槽在内存中，**此后没有自动保存**会把它写出。要观察 `+\r\n`，再走一条 `SaveGuildInfoFile` 生产路径（1038/1039/1040/1041 任一），并同时断言 `bo_changed=false`、`dw_save_tick` 不被 check_save 改动。 |
| 1-3 | W2 用例 C：`now=150/151` 直接 `step` | 10s 门 `now - run_tick > 10000` 不通过 ⇒ `guilds.run` 根本不会跑。每步前显式 `*run_tick = now_tick - 10_001`（§6 W2-C）。 |
| 1-4 | W1 fixture `A,B,C`，断言"名单序非字母序" | `[A,B]` 与字母序重合，无牙。改 `GuildList.txt = B\r\nA\r\nC\r\n` ⇒ 存活表 `[B,A]`、回写 `B\r\nA\r\n`（§6 W1）。 |
| 1-5 | W1 断言"六键段头来自 `from_strings`" | 六键不是同一类：4 个 **段头**（`GuildNotice/GuildWar/GuildAll/GuildMember`）进 `GuildFileSections`；2 个 **封号**（`GuildChief/GuildMemberRank`）进 `GuildConfig`。v2 的 `from_strings(notice,war,all,member)` 只有 4 参，不能承载后两键。分段断言见 §6 W1-6b / W3-0 / W3-1038b。 |
| 1-6 | W3 m7「新建会路径创建 `<名>.txt` ⇒ 红」 | 没有对应用例 ⇒ 不可执行。补 **W3-0 add_guild**（世界层 `add_guild` + 生产 `save_one`/`save_list` 组合，照 `GuildManager.AddGuild` 尾部）。（NPC `GuildOfficial.ReQuestBuildGuild` 尚未移植，登记为跟踪缺口，见 §9-14。） |
| 1-7 | W3 自删解散只有 m3（漏 `save_guild_list`） | 补 **m3b**：漏 `save_one` ⇒ `<名>.txt` 仍是旧 `#1/+掌门` 体 ⇒ 红。 |
| 1-8 | 104 未建模 | 补 §2：条件、文本、帧形、收件人、前缀/颜色、与清槽的先后、`SS_208`。 |
| 1-9 | `[Setup] ShowPreFixMsg` 未列入行会写集 | 该键 `config.rs` 已解析（`GateConfig.show_pre_fix_msg`，用于滚动公告），G3 **复用**到 104；`[String] GuildMsgPreFix` 必须新解析（`strings.rs`）。 |
| 1-10 | 收件人未过 `BanGuildChat` 门 | `GuildInfo.SendGuildMsg` 只在 `guildMember.BanGuildChat == true` 时发（字段名反向，默认 true）。W4-m10 钉住。 |
| 1-11 | `on_disconnect` 在 `Closed` 的调用位置：party 块之后 | oracle 的 104 在 `ProcessPlayObjectData`（`Message.cs:367-389`），`Disappear`（party→guild）在 `ProcessHumans`。可观测约束只有"104 早于清槽、且早于交易取消帧"。v3 规定：`on_disconnect`（104→check_save→清）放在 `deal_cancel_between` **之前**；party 与 guild 清理的相对序不可观测（guild 清理不发帧）。 |
| 1-12 | 同一会话可能被清两次 | `Closed` 会在 `drain_kicks`/`drain_soft_close`/`CM_SOFTCLOSE` 之后再来。加 `SessionGround.guild_done`，四处各跑一次并置位；否则第二次 `DelHumanObj` 会再清一个 `Compare==1` 成员（§3）。 |

---

## 2. 断线 104 退出提示规格（v3 新增）

### 2.1 oracle 出处与条件

`PlayObject.Message.cs:367-389`（`ProcessPlayObjectData` 尾段）：

```csharp
if (BoEmergencyClose || BoKickFlag || BoSoftClose) {
    if (SwitchData) { ... }            // 换服搬家，G3 不涉及
    MakeGhost();
    if (BoKickFlag) SendDefMessage(Messages.SM_OUTOFCONNECTION, 0, 0, 0, 0);
    if (!BoReconnection && BoSoftClose) {
        MyGuild = SystemShare.GuildMgr.MemberOfGuild(ChrName);   // 按名查，不是会话缓存
        if (MyGuild != null) {
            MyGuild.SendGuildMsg(ChrName + " 已经退出游戏.");
            SystemShare.WorldEngine.SendServerGroupMsg(Messages.SS_208, M2Share.ServerIndex,
                MyGuild.GuildName + '/' + "" + '/' + ChrName + " has exited the game.");
        }
    }
}
```

要点：

1. **触发式**：`!BoReconnection && BoSoftClose`（不是"只要断线就发"）。
2. **按名查会**：`MemberOfGuild(ChrName)`（`GuildManager.cs:110-120`，首个 `IsMember` 命中）；会话里的 `MyGuild` 不参与这条。
3. **`MakeGhost()` 之后**：`SendMsg` 有 `!Ghost` 门（`BaseObject.Message.cs:145`），所以**离场者自己收不到**（它已 Ghost）；其他在线成员正常收。
4. **扇出早于清槽**：`DelHumanObj` 在 `PlayObject.Disappear()`（`Message.cs:2138-2142`），由 `ProcessHumans` 在 `PlayObjectList.Remove` 后调用；与 104 不在同一函数，甚至不同定时器（`UserProcessor` 50ms vs 游戏循环）。**本仓取确定序：104 → CheckSave → 清槽**；oracle 的并发窗口登记 T-9x（§9-9）。

### 2.2 `SendGuildMsg` 的文本与门

`GuildInfo.cs:431-460`：

```csharp
if (SystemShare.Config.ShowPreFixMsg) sMsg = SystemShare.Config.GuildMsgPreFix + sMsg;
for each rank, each member:
    if (member.PlayObject != null && member.PlayObject.BanGuildChat)
        member.SendMsg(member, RM_GUILDMESSAGE, 0, GuildMsgFColor, GuildMsgBColor, 0, sMsg);
```

- 文本：`ChrName + " 已经退出游戏."`（名字与"已经"之间**一个半角空格**）。
- 前缀：`ShowPreFixMsg` 为真时 `GuildMsgPreFix + 文本`，**无空格**；`GuildMsgPreFix` 缺省 `〖行会〗`（`GameSvrConf.cs:1843`），来自 **String.conf `[String]`**（`StringConf.cs:38`）。
- 颜色：`[Setup] GuildMsgFColor/GuildMsgBColor`，缺省 `0xDB/0xFF`（`GameSvrConf.cs:1542-1543`，`ServerConf.cs:335-336`）。
- 收件门：`BanGuildChat == true`（**反向命名**，`PlayObject.cs:1007` ctor 置 true = 允许；本仓 `GuildSession.ban_guild_chat` 同义）。
- 空文本不发（`SendSocket(defMsg, sMsg)`，`PlayObject.Socket.cs:108-111`）；本路径文本恒非空。

### 2.3 帧形（复用现有编码器）

`guild_frames::sm_guild_message_104(recipient_actor_id, f, b, text)`（`guild_frames.rs:278-296`）：

- `Recog = 收件人自己`的 ActorId（`SendMsg(baseObject=guildMember, …)` ⇒ `ActorId = baseObject.ActorId`）；
- `Param = MakeWord(GuildMsgFColor, GuildMsgBColor)`；
- `Series = 1`；尾 = `EncodeString(GBK 文本)`；空尾不发。
- 既有用例 `sm_guild_message_104_recog_is_recipient_and_series_one` 已锁住编码；W4 锁的是**接线与收件人集**。

### 2.4 `SS_208`

`SendServerGroupMsg(SS_208, ServerIndex, "<GuildName>//<ChrName> has exited the game.")`（单服无接收端，同 `SS_206/207`）。G3 只把它落成事件/日志：
`GuildServerEvent::ServerGroupMsgPayload { ident: 208, payload: String }`（现有 `ServerGroupMsg { ident, guild }` 承载不了完整 payload，需加变体——**推荐加变体**）。真实线上多服才有接收端，登记 T-9x。

### 2.5 每条宿主路径的输入（条件表）

| 宿主路径 | oracle 置位 | 104？ | 依据 |
|---|---|---|---|
| `GateEvent::Closed`（网关 socket 关闭） | `ChannelMessageHandler.CloseUser`：`if (!OffLineFlag) BoSoftClose = true` | `!bo_reconnection`（默认 true） | `ChannelMessageHandler.cs:390-394` |
| `drain_kicks`（`KickOnlineUser` / 脚本 `KICK` / 重复进世界踢旧） | 只置 `BoKickFlag` | **否**（`BoSoftClose=false`） | `WorldServer.cs:1051-1061`、`NormNpc.cs:236-241` |
| `drain_soft_close`（账号级顶号 `KickUser`） | `BoEmergencyClose=true, BoSoftClose=true`（`BoReconnection` 不动） | `!bo_reconnection`（默认 true） | `TCPNetChannel.cs:266-271` |
| `CM_SOFTCLOSE(1009)` | `BoReconnection=true, BoSoftClose=true`（`Series==1` 再置 `BoEmergencyClose`） | **否**（`!BoReconnection` 假） | `PlayObject.Message.cs:874-884` |

`Closed` 与 `drain_soft_close` 的 Rust 宿主必须**先**按 oracle 把会话的 `bo_soft_close` 置真（`CloseUser`/`KickUser` 语义），再交 `on_disconnect` 判 `!bo_reconnection && bo_soft_close`；`drain_kicks` 保持 false；`CM_SOFTCLOSE` 已在命令里置 `bo_reconnection=true`。**禁止**把"是否发 104"写成路径分支常量——统一由这两个 flag 判，变异才可咬。

---

## 3. `CM_SOFTCLOSE` 生命周期（v3 新增，第四条接线）

oracle 链：

```
CM_SOFTCLOSE: if (!OffLineFlag) { BoReconnection=true; BoSoftClose=true; if (wParam==1) BoEmergencyClose=true; }
  → 同一 tick 的 RunNotice/ProcessPlayObjectData 走 BoSoftClose 支 ⇒ MakeGhost()
  → (!BoReconnection && BoSoftClose) 假 ⇒ 不发 104
  → ProcessHumans 看到 Ghost ⇒ PlayObjectList.Remove; Disappear()
      → Party DelMember → MyGuild.DelHumanObj(this)
          → CheckSaveGuildFile() → 清首个 Compare==1 槽
  → DealCancelA → SaveHumanRcd → CloseUser
```

Rust：`handle_soft_close_cmd` 已做三道置位 + `world.soft_close()`（`main.rs:3495-3548`）。G3 在 `world.soft_close` **成功之后**调用
`guild_runtime::on_disconnect(..., bo_reconnection=true, bo_soft_close=true, ...)`：无 104，做 check_save + 清槽。

**只清一次**：`SessionGround` 增 `guild_done: bool`（显式 `bool`，不用 `Option`，避免与"没行会"混淆）。

- 四处宿主调用点：调用前查 `!sg.guild_done`，调用后置 `true`；
- `Closed` 在 `ground_sessions.remove` 之前判；`drain_kicks`/`drain_soft_close` 各自置；`CM_SOFTCLOSE` 也置；
- `drain_kicks`/`drain_soft_close` 会 `ground_sessions.remove`，后到的 `Closed` 取不到会话 ⇒ 天然 no-op；真正的双清风险在 **`CM_SOFTCLOSE` 后 socket 再关闭**这一路（会话仍在）。
- 重进世界会新建/覆盖 `SessionGround` ⇒ `guild_done=false`，不受影响。

W4-m11 用成员 `[gm,a,b,bb,c]` 咬：第一次清 `b`，若第二次再清则 `bb` 被清（`Compare("bb","a")=1`）。

---

## 4. 时钟域（v2 §2 收敛，仍有效）

- 唯一时钟：`world.clock_ms as i32`（未注入从 0 起、每 tick +200；`world-bevy/src/tick.rs:60`）。
- `guild_cmd::handle_guild_cmd` 的 `now_ms: i64` 改 `now_tick: i32`；调用点传 `world.clock_ms as i32`；`Ctx.now_tick` 直收；13 条 G2 测试改签名。
- `guild_runtime::startup` 调用点把 `guild_run_tick` 设为 `world.clock_ms as i32`（**加载时**，不是 0）。
- `step` 的门与所有落盘 `now` 只用传入 `now_tick`；`on_disconnect` 同。
- 源码锚点：`handle_guild_cmd(` 实参含 `world.clock_ms as i32`，且 `guild_cmd.rs` 无 `now_ms` 标记。
- 首跳：未注入（origin=0）⇒ 10200/20400 不触发 30s 保存，30600 首个 `CheckSaveGuildFile`；origin=3_600_000 ⇒ 3_610_200 首个。
  原因：加载时 `GuildConf.LoadConfig` 经 5 个 property setter 把 `BoChanged` 置真（`GuildInfo.cs:1090-1117`），`DwSaveTick` 保持 0（`GuildState::set_conf` 同）。
- 窗口用 `MIR2_CLOCK_MS=3600000`（>30000、正数）；禁用 epoch 量级（`as i32` 回绕）。
- C# 时钟注入仍未授权 ⇒ §10 B1-B4 `window_required`。

---

## 5. `Compare(...) == 1`（v2 §3 收敛，仍有效）

- .NET 8 实测：返回**码位差**（b/a=1、c/a=2、z/a=25、ab/a=1、a/ab=-1、B/a=1、丁/一=1、中/一=45、会乙/会甲=-9945、b/b=0）。
- 命中当且仅当：(A) 首个大小写折叠后不同的 UTF-16 码元差恰为 1；或 (B) `a` 以 `b` 为前缀（忽略大小写）且码元数恰多 1。
- 实现：`shared/hutil32.rs` 新增 `dotnet_compare_ordinal_ignore_case(a,b) -> i32`（`encode_utf16()` + `to_upper_char` 逐码元；前缀走长度差）。`guild.rs` 的 `del_human_obj_member` 用 `== 1`；**禁止** `cmp_ignore_case(...).cmp == Ordering::Greater`。
- `cmp_ignore_case` 仍供 `FindGuild`/`GetRankName`/`CancelGuld` 的"只判等"使用，不改语义。
- 空槽：`Compare("",name) = -len ≠ 1`，与 oracle `null` 的 -1 同判；`""`/`""` = 0。
- 已知分叉 `ı`/`ß`（`to_upper_char` vs .NET invariant）GBK 不可编码，T-9x。

C7 矩阵（12 行）同 v2；变异：`==1→!=0/>0` ⇒ c/a、z/a、中/一 红；`==1→==0` ⇒ b/a 红；去长度分支 ⇒ ab/a、aa/a 红；`Ordering::Greater` ⇒ c/a 红。

---

## 6. 修正后的判据与变异点

通用：行为层只调生产接缝（`guild_runtime::{startup,step,on_disconnect}`、`handle_guild_cmd`），影子目录读文件字节；保鲜层用 `include_str!("main.rs")` 锚点（照 `main.rs:17754` 的写法：整行 `trim()` 等值 + 实参逐行形状；`//`/`/*` 前缀自动不命中）。`main.rs`/`guild.rs` 已核 `i/lf w/lf`。

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
| 6b | **配置**（2 封号）：`EnterConfig` 构造出、并送给 `step`/`on_disconnect`/`handle_guild_cmd` 的同一份 `GuildConfig` 中 `guild_chief=="掌门X"`、`guild_member_rank=="职位X"`；`sections` 四字段同 fixture。**注意**：这两键是封号不是段头，行为断言在 W3-0/W3-1038b | W3-m8/m9 |
| 7 | 存活会见表物化顺序与 `GuildList` 一致；`bo_changed==true`、`dw_save_tick==0`（`set_conf` 5 setter 置脏） | 若加载后不置脏 ⇒ W2 30600 不保存（W2-m7） |

**W1 变异**：m1 删/注释 `guild_runtime::startup(` 调用行 ⇒ 锚点红；m2 删 `store.load_guild_info` ⇒ 断言 1 红；m3 见 6a；m4 见 4；m5 见 3；m6 见 5；m7 见 2；m8 加载后按名排序 ⇒ 断言 1 红。

### W2 `g3_tick_gate_and_30s_wiring` — 10s 门 / 战争到期 / 30s 门

**驱动**：`guild_runtime::step(guilds, store, sections, msg_cfg, server_index, now_tick, run_tick: &mut i32) -> StepOutcome`。

生产语义（`SystemProcessor.cs:70-77` + `GuildManager.cs:145-167`）：

```
if now_tick.wrapping_sub(*run_tick) > 10_000 {
    *run_tick = now_tick;
    for ev in guilds.run(now_tick) {          // 逐会：倒序扫战表 → WarEnded* → Update 保存 → CheckSave
        WarEnded{message}  ⇒ 先扇出 104（§2.2 同款管道，收件人为会内在线且 BanGuildChat）
        Save{reason}       ⇒ save_one
    }
}
```

`GuildSaveReason ∈ {UpdateGuildFile, CheckSaveGuildFile}`；`StepOutcome { ran, events: Vec<...>, save_results }` 只用于可读，不改 oracle 语义。`save_one` 返回 `Result<SaveStatus, GuildStoreError>`；**调用方**把 `MissingSaveTarget`/其它写错误一律吞掉并记 `保存行会信息失败!!! <path>`（`GuildInfo.cs:425-428`），不 panic、不重试（`save_one` 本身不吞，否则 W3-0 变体 2 不可断言）。

**用例 A（origin=0）**：加载后 `bo_changed=true, dw_save_tick=0, run_tick=0`；把内存 notices 改成 `["N2"]`（文件是 `N1`，使保存字节可见）。`step` 依次 `10000/10200/20400/30600`：

- `10000`：`ran=false`（严格 `>`）；
- `10200`：`ran=true`，无 WarEnded、无 Save（`10200>30000` 假）；
- `20400`：无 Save；
- `30600`：`Save{CheckSaveGuildFile}`；文件出现 `+N2`；`bo_changed=false`；`dw_save_tick` 仍 0。
- 写侧段头：保存后的文件首行 `公告X`，且含整行 `敌对X`/`联盟X`/`成员段X`（4 键写侧；与 W1-6a 的读侧互补）。

**用例 B（origin=3_600_000）**：`run_tick=3_600_000, dw_save_tick=0`。首个 `step(3_610_200)`：门 `10_200>10_000` 真、`3_610_200>30_000` 真 ⇒ `Save{CheckSaveGuildFile}`；`bo_changed=false`。`step(3_620_400)`：门真但 `bo_changed=false` ⇒ 无 Save。

**用例 C（战争到期 + 事件序）**：一会两战 `E1{war_tick:100, war_time:50}`、`E2{war_tick:1000, war_time:50}`，成员 `[b,c]` 在线。
**每步前显式** `*run_tick = now_tick - 10_001`：

- `now=150`：`*run_tick=-9851`；门过；`run` 判 `150-100=50>50` 假 ⇒ 无 WarEnded、无 Save；
- `now=151`：`*run_tick=-9850`；门过；`151-100=51>50` 真 ⇒ 事件序 = `WarEnded(E1)`（104 文本 `***E1行会战争结束`，前缀/颜色同 §2.2）**先**、`Save{UpdateGuildFile}` **后**；无 `CheckSaveGuildFile` Save（`dw_save_tick=151`）。文件里 E1 行消失、E2 行保留为 `+E2 899`（`50-(151-1000)`）。`bo_changed` 仍 true（`UpdateGuildFile` 置真，`CheckSave` 因 `151-151` 不触发，故不清）。

**W2 变异**：m1 删/注释 `guild_runtime::step(` 调用行与 `guild_run_tick` 赋值 ⇒ 锚点红；m2 `>`→`>=` ⇒ 用例 A 的 10000 `ran=true` 红；m3 删 `guilds.run` ⇒ 用例 C 无 WarEnded、E1 留文件 红；m4 删 Save 应用 ⇒ 用例 A `+N2` 不出现 红；m5 调用点传 `now_ms()` ⇒ 锚点红 + 行为种子（`1_700_000_000_000i64 as i32`）使 30s 门永不触发 红；m6 事件序把 Save 排到 WarEnded 前 ⇒ 用例 C 红；m7 把 `bo_changed=false` 提前 ⇒ 用例 B 第二跳再 Save 红；m8 不扇出 WarEnded 104 ⇒ 用例 C 收件人空 红；m9 前缀/颜色/Series 写错 ⇒ 用例 C 帧断言红。

### W3 `g3_change_point_saves_wired` — 变更点立即落盘（含 W3-0 add_guild）

**rig**：`handle_guild_cmd`（G2 生产入口，`guild_cmd.rs:198`）扩参 `store/sections/msg_cfg/server_index`，`now_tick: i32`。影子目录；先 `startup` 后 `step(31_000)` 让 `bo_changed=false`（`31_000>30_000`，`run_tick=0`），再在 `now=40_000` 发命令。每个用例结束对比文件字节 + 状态。

| CM / 用例 | 断言（字节 + 状态） |
|---|---|
| 1038 成功 | `<名>.txt` 增加 `+target`（若目标为 rank99 首个，还增 `#99 职位X` 行）；`bo_changed=true`；`dw_save_tick==40000` |
| 1038b 无 rank99 的会 | 文件新增整行 `#99 职位X`（**GuildMemberRank 行为牙**） |
| 1039 删他人 | 文件少 `+target`；`bo_changed=true`；`dw_save_tick==40000` |
| 1039 自删解散（fixture：该会只有 rank1 + 掌门一人，命令由掌门本人发） | `<名>.txt` = 空会 7 行体（`公告X\r\n \r\n敌对X\r\n \r\n联盟X\r\n \r\n成员段X\r\n`，**文件仍存在**）；`GuildList.txt` 不含该名；`.bak` 与 `<GuildDir>/<名>/` 都不存在；`bo_changed`/`dw_save_tick` 不变 |
| 1040 公告 | 公告段逐字节更新；`bo_changed` 保持 false；`dw_save_tick` 保持 0（直写 `SaveGuildInfoFile`，不经 `UpdateGuildFile`） |
| 1041 成功 | ranks 已清（N3）⇒ 文件无 `#` 行；`bo_changed=true`；`dw_save_tick==40000` |
| 1044/1045 | 生产恒失败（769/-4、769/0）；无状态变化、无新写；文件字节不变 |
| **W3-0 add_guild（新增）** | **变体 1**：预存 `<新会>.txt`（旧体）⇒ 按 ctor 先 `load_guild_conf` 建 `.ini`，再 `add_guild` + `save_one`，之后文件 = 空会体 + `#1 掌门X` + `+chiefX`（**GuildChief 行为牙**）；`.ini` 0 字节；`GuildList.txt` 增该名；**新会** `bo_changed=false`、`dw_save_tick==0`（`SetGuildInfo` 不置脏；加载来的旧会仍是 `bo_changed=true`）。**变体 2**：`<新会>.txt` 缺失 ⇒ `save_one` 返回 `MissingSaveTarget`（吞掉）、`<新会>.txt` **仍不存在**、不 panic；`GuildList.txt` 仍增该名（`SaveGuildList` 在 `SetGuildInfo` 之后） |

**W3 变异**：m1 1038 删 `save_one` ⇒ 无 `+target` 红；m2 1039 删 `save_one` ⇒ 仍含 `+target` 红；m3 自删删 `save_list` ⇒ 名单仍含该名 红；**m3b 自删删 `save_one`** ⇒ txt 仍是旧 `#1/+GM` 体 红；m4 1040 改用 `update_guild_file` ⇒ `bo_changed/dw_save_tick` 变 红；m5 任一臂用 `now_ms()`（epoch `as i32`）⇒ `dw_save_tick` 不等 红；m6 写 `.bak` 或建 `<名>/` 子目录 ⇒ 红；m7 保存侧改成"缺文件也创建" ⇒ W3-0 变体 2 文件出现 红（**本条靠 W3-0 才可执行**）；m8 `add_guild` 用缺省 cfg ⇒ `#1 掌门人` 而非 `掌门X` 红；m9 1038 建 rank99 用缺省 ⇒ `#99 行会成员` 红。

> 写次数口径（不变）：自删解散 oracle 写空体两次（`CancelGuld` + `DelGuild` 各一次 `BackupGuildFile`），字节相同 ⇒ 不可观测；只断字节与名单。登记 T-9x。

### W4 `g3_disconnect_wiring` — 四条生命周期 + 104

**驱动**：`guild_runtime::on_disconnect(guilds, store, sections, msg_cfg, server_index, ctx, recipients) -> DisconnectOutcome`：

```rust
// ctx: guild_name: Option<&str>, chr_name, bo_reconnection, bo_soft_close, now_tick
// recipients(name) -> Option<(actor_id, ban_guild_chat_allow)>（在线且非 Ghost；排除离场者）
// 1) 104（oracle 先发）：
//    if !ctx.bo_reconnection && ctx.bo_soft_close {
//        if let Some(g) = guilds.member_of_guild(ctx.chr_name) {   // 按名查，不是会话
//            for rank in g.ranks { for m in rank.members {
//                if let Some((aid, allow)) = recipients(m) { if allow {
//                    out.notices.push((aid, sm_guild_message_104(aid, f, b, prefixed(text))))
//                }}}
//            out.ss208 = Some(format!("{g}//{} has exited the game.", ctx.chr_name))
//        }
//    }
// 2) 若 Some(g) = guild_name.and_then(find_guild_mut)：
//    if g.check_save_guild_file(now_tick) { out.save = save_one(...); }   // 先落盘（BoChanged=false，DwSaveTick 不动）
// 3) 再 g.del_human_obj_member(chr_name)                                   // Compare==1 清首个
```

**四条宿主接线（缺一红；每条都查/置 `guild_done`）**：

1. `GateEvent::Closed`（`main.rs:15057`）：`engine.delete_human(socket)` 后、**`deal_cancel_between` 之前**：
   捕获 `sg.bo_reconnection`，按 `CloseUser` 置 `sg.bo_soft_close=true`，取 `guild_name`（`ground_sessions.get(...)`），
   调 `on_disconnect`；帧按 `SimToGate::SendFrame` 路由；随后照旧 `online_take`/存档/移除/party/`world.disconnect`。
2. `drain_kicks`（`main.rs:11812`）：`world.disconnect` 前调；`bo_soft_close=false`（oracle 只置 `BoKickFlag`）⇒ 无 104。
3. `drain_soft_close`（`main.rs:11944`）：`handle_kick_account` 里补置 `sg.bo_soft_close=true; sg.bo_emergency_close=true`（照 `KickUser`）；落地时调 ⇒ 有 104。
4. `handle_soft_close_cmd`（`main.rs:3495`）：`world.soft_close` 成功后调；`bo_reconnection=true` ⇒ 无 104，但 check_save + 清槽（§3）。

**断言**（fixture：会 G = rank1 `#1 掌门X`+`+gm`、rank99 `#99 职位X`+`+a +b +bb +c`；文件 notices `N1`、内存 `N2`、加载后 `bo_changed=true, dw_save_tick=0`；`msg_cfg`：`ShowPreFixMsg=1`、`GuildMsgPreFix=〖行会X〗`、`GuildMsgFColor=0x0A`、`GuildMsgBColor=0x0B`。`gm` 是掌门，用来在断线后再发 1040）：

| # | 断言 |
|---|---|
| W4-1 | `now=40000` 断 `a`（`bo_soft_close=true, bo_reconnection=false`）：文件**仍有** `+gm/+a/+b/+bb/+c`、**无** `+\r\n`、出现 `+N2`（check_save 确实落盘）；`bo_changed=false`；`dw_save_tick` **仍 0**；内存 `b==""`、`count()==5`、`is_member("b")==false`、`is_member("")==false`、`bb`/`c` 不动 |
| W4-2 | 104 发到 `gm,b,bb,c`（**不发** `a`）；每帧 `Recog=收件人`、`Param=MakeWord(0x0A,0x0B)`、`Series=1`、尾 `EncodeString(GBK("〖行会X〗a 已经退出游戏."))`（`ShowPreFixMsg=1`）；发送序 = rank/member 序 |
| W4-3 | 随后 `handle_guild_cmd(1040, "N3")`（由 `gm` 发，`now=41000`）⇒ 文件出现 `+\r\n`（b 的空槽）与 `+N3`；`bo_changed` 仍 false、`dw_save_tick` 仍 0 ⇒ **清槽在内存且由下一次直写带出**（不是"自动下一次保存"） |
| W4-4 | `bo_reconnection=true`（`CM_SOFTCLOSE` 路径；或带该标志的 `Closed`）⇒ **无 104**，但 check_save+清槽各发生一次（重复调用见 W4-8） |
| W4-5 | `bo_soft_close=false`（`drain_kicks`）⇒ **无 104**；清槽仍发生 |
| W4-6 | `guild_name=None` ⇒ 完全 no-op（无 104、无 save、无清） |
| W4-7 | 成员 `[a,c]` 断 `a` ⇒ `c` 不动（`Compare=2`）；成员 `[a]` 断 `a` ⇒ 不动（`Compare=0`）；成员 `[b,c]` 断 `c` ⇒ `b` 不动（`-1`） |
| W4-8 | `CM_SOFTCLOSE` 后再来 `Closed` ⇒ `bb` 仍在（不二次清；`guild_done`；成员表 `[gm,a,b,bb,c]`：第一次清 `b`，第二次会清 `bb`） |
| W4-9 | 收件人 `ban_guild_chat=false` 的成员不收款 |
| 锚点 | `main.rs` 四条调用行 + 实参形状（`guild_name` 在 `ground_sessions.remove` 之前取；`on_disconnect` 在 `deal_cancel_between` 之前；`handle_soft_close_cmd` 在 `world.soft_close` 之后） |

**W4 变异**：m1 删 `Closed` 的 `on_disconnect` ⇒ 锚点 + W4-1 红；m2 `guild_name` 取在 `ground_sessions.remove` 之后 ⇒ 恒 None ⇒ W4-1/2 红；m3 先清后 check_save ⇒ W4-1 文件出现 `+\r\n` 红；m4 不调 check_save / 忽略返回值 ⇒ `+N2` 不出现且 `bo_changed` 仍 true 红；m5 删 `drain_kicks` 调用 ⇒ 锚点红（行为无 104 不红，锚点是唯一牙）；**m5b 删 `handle_soft_close_cmd`/`drain_soft_close` 调用 ⇒ 对应锚点红 + W4-4/W4-8 红**；m6 用 `Ordering::Greater` ⇒ W4-7 的 `[a,c]` 红（c 被清）；m7 104 在清后 ⇒ `b` 不收 ⇒ W4-2 红；m8 104 恒发 ⇒ W4-4/W4-5 红；m9 条件恒假 ⇒ W4-2 红；m10 忽略 `BanGuildChat` ⇒ W4-9 红；m11 去掉 `guild_done` ⇒ W4-8 红；m12 不拼前缀/前缀空 ⇒ W4-2 尾红；m13 颜色互换或 `Series` 非 1 ⇒ W4-2 红。

### 其余判据去向（同 v2）

| 原编号 | 去向 |
|---|---|
| T2 文件字节 | C2；落盘统一 `guild_runtime::save_one`，主循环/变更点各一条锚点 |
| T3 10s 严格 `>` | W2-A/C |
| T5 战争到期事件序 | W2-C |
| T7 Compare | §5 C7 |
| T9 绑定非在线 / 104 | C9 + W2-C（WarEnded 104）+ W4（退出 104） |
| T10 104 线形 | C10；`Recog=ActorId`、`Param=MakeWord(F,B)`、`Series=1`、尾 GBK |
| T11 非 0 区 | 可选 |

---

## 7. 写集与串行顺序

| # | 文件 | 改动 |
|---|---|---|
| 1 | `mir2-rs/crates/shared/src/hutil32.rs` | `dotnet_compare_ordinal_ignore_case`（+ 单测矩阵；复用 `to_upper_char`） |
| 2 | `mir2-rs/crates/world/src/guild.rs` | `del_human_obj_member`（纯清，Compare==1）/ 删旧 `del_human_obj`；`run -> Vec<GuildRunEvent>`（WarEnded + Save{reason}，保留逐会倒序与 `Update→CheckSave` 次序）；`GuildSaveReason`；`to_record`/`names`/`del_guild -> Option<GuildRecord>`；`ref_member_name` 保持；改 3 条既有单测（`:1372`、`:1423`、`:1820`）与 `run` 两条 |
| 3 | `mir2-rs/crates/data/src/guild.rs` | `GuildFileSections::from_strings(notice: &str, war: &str, all: &str, member: &str)`（4 段头；**不能**收 `bins` 的 `GuildStrings`，避免 data→bins 反向依赖；保留 `from_config` 但 G3 不用） |
| 4 | `mir2-rs/bins/game-svr/src/guild_runtime.rs`（新建） | `startup`/`step`/`on_disconnect`/`save_one`/`save_list`/`guild_broadcast`；`StepOutcome`/`DisconnectOutcome`；只做接线 |
| 5 | `mir2-rs/bins/game-svr/src/guild_cmd.rs` | `now_tick: i32`；`Ctx` 增 `store/sections/msg_cfg/server_index`；1038/1039/1040/1041 落盘；13 条 G2 测试改签名 |
| 6 | `mir2-rs/bins/game-svr/src/main.rs` | `EnterConfig` 增 `guild_store/guild_sections/guild_msg_cfg`（注释 `from_config`→`from_strings`，`main.rs:12283-12287`）；`sim_loop` 加载 + `guild_run_tick` + `step`；**四条**断线 + 104 + `guild_done`；`handle_guild_cmd` 传 `world.clock_ms as i32`；锚点用例 |
| 7 | `mir2-rs/bins/game-svr/src/strings.rs` | `[Guild]` 六键 + `[String] GuildMsgPreFix`（缺省 `〖行会〗`）；section 解析由单 bool 改 `Option<Section>` |
| 8 | `mir2-rs/bins/game-svr/src/config.rs` | `[Setup] GuildMsgFColor/GuildMsgBColor`（缺省 `0xDB/0xFF`）；`ShowPreFixMsg` 已有，接到行会 msg_cfg |
| 9 | `mir2-rs/tests/parity/whitelist.md` | T-9x 登记（§9） |

不碰：`src/**`、`sql/**`、`MirClient/`、`crates/protocol`、`crates/storage`、`E:/MirServer`、`/e/tmp/wup`、`tools/`。

串行：S1 shared+world → S2 data → S3 strings/config → S4 guild_runtime → S5 guild_cmd → S6 main → S7 whitelist/门禁。理由同 v2（API 基础先行；`main.rs` 最后动，压一次 rebase）。

---

## 8. 更新后的 G3 DoD

- [ ] S1-S7 完成；`git status` 干净；`git diff --stat <基线> -- src/ sql/ MirClient/ crates/protocol crates/storage` 为空；
- [ ] 四门禁全绿（用本车道 `CARGO_TARGET_DIR`）：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p msgcodegen -- --check`；
- [ ] W1：`B,A,C` 顺序牙；4 段头**读+写**；`GuildChief/GuildMemberRank` 配置断言（6b）；
- [ ] W2：用例 A/B/C；C 每步 `*run_tick = now-10001`；写侧 4 段头；WarEnded 104；
- [ ] W3：1038/1038b/1039/1039 自删/1040/1041/1044/1045；**W3-0 add_guild 两变体**（`#1 掌门X`、缺文件不建）；自删空体 7 行；
- [ ] W4：四处宿主接线；104 条件表（发/不发四路）；104 早于清槽；`CheckSave` 不推 `DwSaveTick`；断线后文件仍是旧体、`+\r\n` 由后续 1040 带出；`guild_done` 单次清理；
- [ ] C7 12 行全绿（含 c/a、中/一、ab/a、会乙/会甲）；
- [ ] 变异日志：§6 每条"注入 → 指定用例红 → 还原（sha256）"有记录；**W3-m7 必须由 W3-0 变体 2 咬红**；
- [ ] §10 构造级全绿；窗口 A 由 owner 执行并留证据；
- [ ] §10 B1-B4 未授权 ⇒ `review` 写 `window_required:true` + 缺项清单；`tier=R` + `tier_why` 点名 `SaveGuildInfoFile`/`CheckSaveGuildFile`/`DelHumanObj`/10s+30s 门/104 时序；
- [ ] T-9x 已写 `whitelist.md`；
- [ ] 收尾：删本车道 `CARGO_TARGET_DIR`；无 worktree（本车道只读）、无进程、无端口、无 `E:/MirServer` 改动。

---

## 9. 白名单 T-9x（编号避让其他车道；v3 新增 9-13，14 为跟踪缺口）

1. **节拍量化**：world tick 200ms vs oracle 50ms 扫描；10s 门首跳 +10200（oracle +10050）、30s 边界最多晚 150ms、战争到期最多晚 200ms；命令读上一 tick 的 `world.clock_ms`（≤200ms）。
2. **未注入首存 30600**；窗口强制 `MIR2_CLOCK_MS=3600000`。
3. **顶号同 tick 清理**：oracle `MakeGhost` → 下一次 `ProcessHumans`（≤50ms）`DelHumanObj`；Rust 同 tick；仅相位差。
4. **保存编码**：Rust 恒 GBK；oracle 按 BOM 嗅探；部署件无 BOM。
5. **加载失败降级**：`.ini` 创建失败 oracle 抛到 `InitializeWorld` catch；Rust 记 ERR、空表。
6. **绑定是加载期快照**：`actor_id` 可能重登复用；极端情况收件人不同。
7. **`to_upper_char` 与 .NET 的 `ı`/`ß` 分叉**（GBK 不可编码）。
8. **自删解散写空体两次**（字节相同，写次数不可观测）。
9. **104 与 `DelHumanObj` 的线程竞态**：oracle 104 在 `ProcessPlayObjectData`、清槽在另一 `UserProcessor` 定时器；并发窗口极小。Rust 取确定序"104→清"，极端并发下 oracle 可能先清后发（收件人少一个）。单服部署不可观测。
10. **`SS_208` 无接收端**：单服只落事件/日志，`GuildName//ChrName has exited the game.` 不上线。
11. **`CheckSaveGuildFile` 的存盘通常是重复写**：`UpdateGuildFile` 已即时落盘；30s 后的 check_save 只重写同字节。`DelHumanObj` 的清槽在其自身 check **之后**且不置脏，故该次 check 也写不出清槽。故用 `N2`（内存）vs `N1`（文件）的种子使保存字节可见；**生产可达的 `bo_changed=true` 来自加载时 `GuildConf.LoadConfig` 的 5 个 setter**。
12. **`.bak` 路径**：oracle `BackupGuildFile` 写 `<GuildDir>/<名>/.<tick>.bak`，父目录不存在 ⇒ `FileMode.Open` 抛错被吞；Rust 不建模写 `.bak`。若部署残留该子目录，oracle 会写而 Rust 不写（登记）。
13. **`GuildCastleConfig.guild_chief/guild_member_rank` 不是生产源**：oracle 从 String.conf `[Guild]` 读（`StringConf.cs:30-35`）；G3 必须在构造后**覆盖**（§6 W1-6b/W3-m8/m9），否则静默回落 `掌门人/行会成员`。
14. （跟踪缺口，非白名单）**`GuildOfficial.ReQuestBuildGuild` 未移植**：W3-0 只锁 `add_guild` + `save_one/save_list` 组合（生产函数），不声称 NPC 调用点已接线；另开卡 `openmir2-m3-guild-create-npc`。

---

## 10. C# 时钟注入授权边界（同 v2）

- §10.1 构造级（无需 C# 授权）：W1/W2/W3/W4 行为 + C2/C7/C9/C10 + 锚点 + 变异注入。
- §10.2 真机窗口 A（不需注入）：C# 开机 >30s；Rust `MIR2_CLOCK_MS=3600000`；两会影子目录。断言首个 10s Run 后两会 `<名>.txt` 重写、战表行消失、公告/成员段逐字节不变、`GuildList.txt` 不变、`.ini` 0 字节、无 `.bak`；掉线用"差 1/差 2"两组观察下一份直写里的 `+\r\n`。
- §10.3 窗口 B（须 C# 时钟注入，blocked）：B1 `dw_save_tick` 绝对值；B2 10s/30s 精确边界；B3 i32 回绕；B4 战争到期逐毫秒对拍。未授权 ⇒ `not_verified` / `window_required:true`。
- §10.4 review 措辞：`tier: R`；只做构造级 ⇒ `approve + window_required:true`；A 完成 B 未完成 ⇒ 仍 `window_required:true`；A+B 全绿才可裸 `approve`。

---

## 11. 复核命令（只读）

```bash
git rev-parse HEAD                                   # ae669377f
sed -n '367,390p'   src/M2Server/Player/PlayObject.Message.cs   # MakeGhost + 104 + SS_208
sed -n '431,460p'   src/M2Server/Guild/GuildInfo.cs             # SendGuildMsg 前缀/颜色/BanGuildChat
sed -n '525,550p'   src/M2Server/Guild/GuildInfo.cs             # CheckSave / DelHumanObj(Compare==1)
sed -n '874,884p'   src/M2Server/Player/PlayObject.Message.cs   # CM_SOFTCLOSE 三道置位
sed -n '2138,2149p' src/M2Server/Player/PlayObject.Message.cs   # Disappear: party → guild
sed -n '390,394p'   src/M2Server/Net/ChannelMessageHandler.cs   # CloseUser 置 BoSoftClose
sed -n '266,271p'   src/M2Server/Net/TCP/TCPNetChannel.cs       # KickUser 置 Emergency/SoftClose
sed -n '1090,1117p' src/M2Server/Guild/GuildInfo.cs             # 5 个 setter 置 BoChanged
sed -n '145,167p'   src/M2Server/Guild/GuildManager.cs          # Run 次序
sed -n '70,77p'     src/GameSrv/Word/Threads/SystemProcessor.cs # 10s 门
sed -n '19,32p;34,52p' src/M2Server/Guild/GuildManager.cs       # AddGuild / DelGuild
sed -n '112,171p'   src/GameSrv/Npc/GuildOfficial.cs            # ReQuestBuildGuild（未移植）
grep -n '"Guild"' src/Modules/SystemModule/Conf/StringConf.cs   # 六键来源
sed -n '17745,17780p' mir2-rs/bins/game-svr/src/main.rs         # 锚点既有写法
```

---

## 12. 残留物

- 仅 `E:/tmp/laneB-guild-g3-v3.md`（本文件）与 v2 原文；`E:/tmp/laneB-cmp-net8/` 探针由前序车道保管（只读）。
- 本车道无 worktree、无 `CARGO_TARGET_DIR`、无编译产物、无进程、无端口、无 `E:/MirServer` 改动。


---

## 评审员更正（2026-10-20，**v3 仍需一轮**）

- **C1（重要，模型错误）**：`PlayObject.Message.cs:382` 在 104 块里会 **`MyGuild = MemberOfGuild(ChrName)`**，而 `Disappear`（`:2146`）
  用的正是这个被覆盖后的 `MyGuild`。oracle 是**一个源**：`!bo_reconnection && bo_soft_close` 时先覆盖会话 `MyGuild`（可为 null）、
  再发 104、再清槽；其它路径不覆盖。v3 把它拆成两个源 ⇒ W4-6 与 m2 的期望要改（仅会话 None、manager 有会时仍发 104 并清槽）。
- **C2（重要，接线缺口）**：生产路径的「排除离场者」只由测试闭包实现——`Closed` 路径在 `online_take`/`world.disconnect` 之前调
  `on_disconnect`，此时离场者仍在 `online` 且尚未 Ghost；oracle 靠 `!Ghost` 门挡住。必须显式排除 `chr_name` 或先做 Ghost，并加锚点/用例。
- **C3（覆盖缺口）**：`SS_208` 只落事件、W4 无断言。
- 其余：104 条件表与文本、帧字段、扇出早于清槽、`BanGuildChat` 门等已实核成立。
