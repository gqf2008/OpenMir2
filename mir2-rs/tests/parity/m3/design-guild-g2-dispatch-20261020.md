# Lane B — 行会会话层 G2/G3 派工简报

- 基线：`d7dfc1055`（master）。本车道**只读**：不建 worktree、不改仓库、不起栈、不碰 `E:/MirServer`，也不碰 `/e/tmp/wdb`、`/e/tmp/wsel` 两条车道。
- 输入：`mir2-rs/tests/parity/m3/design-guild-session-layer-20261019.md`（487 行）、已落地 G1 `mir2-rs/bins/game-svr/src/guild_frames.rs`（640 行，15 帧 + 771 反向判据）、oracle `src/M2Server/Player/PlayObject.Operate.cs:1229-1560` / `GuildInfo.cs` / `GuildManager.cs` / `SystemProcessor.cs:71-77`。
- 本文件所有行号为 `d7dfc1055` 实测；`main.rs` 已到 44460 行，设计件的旧行号只能当锚点不能照抄。

---

## 0. 开工前先钉死的 6 条口径（与设计件不一致处，以本节为准）

1. **`CM_GUILDUPDATERANKINFO(1041)` 成功 = 零帧，不是 756**。oracle `UpdateRank` 成功时 `RankList = rankListTmp`（空表，N3，`GuildInfo.cs:967-971`）⇒ 随后的 `ClientGuildMemberList` 拼出空串 ⇒ `MakeMessage + SendSocket(defMsg, "")` 在 `PlayObject.Socket.cs:108-111` **整帧不发**。设计件 §1.2/§2 写的「`nC==0` ⇒ `SS_207` + `756`」应改为「`SS_207` 事件 + **0 帧**」。这条同时解释 `guild_castle_scenario.json` 的 `guild_member_ops`：`update_rank` 后 `del_member peer` 必走 `760/2`（ranks 已被 N3 清空，`IsMember` 为 false）。
2. **`CM_GUILDBREAKALLY(1045)` 失败发 `769 Recog=0`，不是 771**（`Operate.cs:1553-1556`）。`771` 全仓无发点，`768/770` 在生产不可达。`guild_castle_scenario.json` 的 `guild_ally_ops.note` 现写「`770/771`」——**夹具注记本身有误**，G3 触夹具时改注记（只改 note，不改比较器）。
3. **`CM_1042` 不进行会臂**：oracle 只记「非法数据」日志（`PlayObject.Message.cs:995-997`），Rust 已在 `inbound::INBOUND_IDENTS`。`CM_ADJUST_BONUS(1043)` 也在 inbound。行会臂用**显式 9 个 ident**，绝不能用 `1035..=1045` 区间（会把 1042/1043 吞掉）。
4. **750 无会登录 = 一条都不发**，队列条目不占堆位（`GuildInfo.GetRankName` 只在 `MyGuild != null` 时 `SendMsg(RM_CHANGEGUILDNAME)`，`GuildInfo.cs:485-509`；登录 `PlayObject.cs:1391-1400`）。`SendChangeGuildName` 的无会纯头只服务「先清 MyGuild 再 `RefRankInfo(0,"")`」的删人/解散路径。
5. **`CM_GUILDHOME(1036)` 就是 `ClientOpenGuildDlg`**（`Operate.cs:1278-1281`），与 1035 完全同构；判据里不要写第二套期望。
6. **`AllowGuild` 生产恒 false、`BanGuildChat` 生产恒 true**：前者唯一写点是 GM `@Letguild`（`LetGuildCommand.cs:12`），本仓 `chat.rs` 未接 `@` 命令 ⇒ 加成员生产恒 `758/5`；后者唯一写点是 GM `@BanGuildChat` 与跨服 SwitchData（`WorldServer.Switch.cs:26/116`，本仓未移植）。二者都仍要建模，判据用测试夹具直接置位，**不得**为跑通成功路径偷偷改默认值。

---

## 1. G2：9 个 CM_GUILD* 的派发插入点

### 1.1 插入位置（唯一一处）

- 文件/位置：`mir2-rs/bins/game-svr/src/main.rs` 的 `sim_loop` 平铺 `else if` 链，**插在 `handle_spell_cmd_with_magic` 之后、`handle_login_notice_ok_cmd`（:14781）之前**，即当前 :14779-14781 之间新增一个臂；inbound 兜底保持在其后（:14790）。
- 形状：

```rust
} else if matches!(cmd.ident,
    mir2_protocol::messages::CM_OPENGUILDDLG
  | mir2_protocol::messages::CM_GUILDHOME
  | mir2_protocol::messages::CM_GUILDMEMBERLIST
  | mir2_protocol::messages::CM_GUILDADDMEMBER
  | mir2_protocol::messages::CM_GUILDDELMEMBER
  | mir2_protocol::messages::CM_GUILDUPDATENOTICE
  | mir2_protocol::messages::CM_GUILDUPDATERANKINFO
  | mir2_protocol::messages::CM_GUILDALLY
  | mir2_protocol::messages::CM_GUILDBREAKALLY)
{
    handle_guild_cmd(/* 见 §1.3 签名 */);
}
```

- 先后关系：oracle 的 `switch` 在 `CM_USERMAKEDRUGITEM` 之后依次 `1035..1041 → 1042(非法) → 1043(adjust) → 1044/1045`（`PlayObject.Message.cs:975-1007`）。Rust 的这些 ident **互斥且不与既有臂重叠**，位置不影响语义；按上述位置插可保持「guild 臂在 inbound 兜底之前」的审计形状。`CM_SAY/PICKUP/DEAL/SELLOFF/BAG/EAT/QUERY/MOVE/PARTY/ATTACK/MAGIC` 都不包含 1035-1041/1044/1045。
- 实现落点：**新建 `mir2-rs/bins/game-svr/src/guild_cmd.rs`**，`main.rs` 只加 `mod guild_cmd;`（:31 附近）+ 一个臂；9 个 handler 全在新文件，避开 PL-1 在 `/e/tmp/pl1` 的 `main.rs` P21/P32 写集。

### 1.2 逐条回包与副作用（顺序即 oracle 语句序）

| # | CM | 守卫/失败码 | 回包（ident, recog, param, tag, series, tail） | 状态/事件副作用 |
|---|---|---|---|---|
| 1 | 1035 `OPENGUILDDLG` | 无 | 有会：`753,0,0,0,1,Encode(panel)`；无会：`754,0,0,0,0,无尾` | 无 |
| 2 | 1036 `GUILDHOME` | 同 1 | 同 1 | 无 |
| 3 | 1037 `GUILDMEMBERLIST` | 无会 ⇒ 早退 | 有会且文本非空：`756,0,0,0,1,Encode(text)`；无会或文本空：**0 帧** | 无 |
| 4 | 1038 `GUILDADDMEMBER` | 1 非掌门；2 目标不在线/没面对；3 已在本会；4 已在别会**或 rank 数≥400**；5 `AllowGuild=false` | 成功 `757` 纯头；失败 `758,Recog=nC,0,0,0` | 成功：`add_member`（建/找 rank 99）→ `SS_207` 事件 → 目标会话 `guild/rank_no/rank_name` → 目标 `750`（尾 `会名/封号`）→ 目标 `42`（RefShowName）→ 目标 `100`（Green，Hint，`你已加入行会: {会名} 当前封号为: {封号}`）；掌门收 `757` |
| 5 | 1039 `GUILDDELMEMBER` | 1 非掌门；2 非本会成员；3 自删（`CancelGuld` 成功则最终 0）；4 `DelMember` 返回 false | 成功 `759` 纯头；失败 `760,Recog=nC` | 删他人：`del_member` → 目标会话清空 → 目标 `750` **纯头** → 目标 `42` → `SS_207`；自删且 `CancelGuld` 成功：`DelGuild` → `SS_206` → 自己会话清空 → 自己 `750` 纯头 → `42` → `100`（**Red**，`行会{名}已被取消!!!`）。注意 `759` 是直发、`750/42/100` 是排队：同一 socket 上 **`759` 先出**，其后再 `750→42→100` |
| 6 | 1040 `GUILDUPDATENOTICE` | `MyGuild==null || rank_no!=1` ⇒ 静默 0 帧 | 成功：`753`（重开面板） | `NoticeList.Clear()` → 按 `GetValidStr3(..., '\r')` 逐段填（`RemoveEmptyEntries` 语义）→ `SaveGuildInfoFile()`（**不经 BoChanged**，G3）→ `SS_207` |
| 7 | 1041 `GUILDUPDATERANKINFO` | 同上；`UpdateRank` 返回 `nC` | `nC==0`：**0 帧**（见 §0.1）；`nC==-1`：0 帧；`nC<=-2`：`761,Recog=nC,0,0,0` | `nC==0`：ranks 被 N3 清空 + `UpdateGuildFile`（G3）+ `SS_207`；`-1` 无副作用；`<=-2` 无状态变化。**会话 rank_no/rank_name 不动**（oracle 不清） |
| 8 | 1044 `GUILDALLY` | `n8` 初值 -1；目标非玩家/无会/没回看 ⇒ -1；`EnableAuthAlly` 恒 false ⇒ **-4**；掌门/战态门 ⇒ -3/-2（当前不可达） | `n8==0`：`768`；否则 `769,Recog=n8` | 成功路径（`AllyGuild`/两条 `SendGuildMsg`/双向 `RefMemberName`/两条 `SS_207`）**生产不可达**；只建模不留半套 |
| 9 | 1045 `GUILDBREAKALLY` | 非掌门 ⇒ 静默；`FindGuild(tail)` 大小写敏感；`IsAllyGuild` 恒 false（联盟表恒空） | 成功 `770`；失败 **`769,Recog=0`**；`771` 永不发 | 成功路径不可达；无状态变化 |

补充：

- 1038 的严格顺序：`IsGuildMaster()` → `GetPlayObject(name)!=null`（**OrdinalIgnoreCase**，且 `!Ghost`、排除 `IsPasswordLocked && ObMode && AdminMode`，`WorldServer.cs:937-953`）→ `target.GetPoseCreate()==this`（**目标面朝掌门**，Rust 复用 `pose_create(&world,&displays,target)`）→ `target.AllowGuild` → `!MyGuild.IsMember(name)`（**大小写敏感** `==`）→ `target.MyGuild==null && MyGuild.RankList.Count<400`（**rank 条数**，`GUILD_RANK_LIST_LIMIT` 首次真正使用）→ 副作用链。
- 1039 的 `ChrName != sHumName` 是**大小写敏感**；自删在多人会里 `CancelGuld` 返回 false ⇒ 最终 `760/3`（不是 4）。删他人的 `DelMember` 用**大小写敏感** `==`；`IsMember` 也是 `==`。
- `SS_205/206/207` 单服无接收端（`WorldServer.cs:885-895`）⇒ G2 建模为 `Vec<GuildServerEvent>` 供日志/判据，**不产客户端帧**。
- 每个 handler 开头都先 `online.iter().find(|e| e.chr_name == chr_name)` + `ground_sessions.get(chr_name)`；缺失记 WRN 丢弃（与 `handle_party_cmd` 同构）。

### 1.3 建议签名（`guild_cmd.rs`）

```rust
pub(crate) fn handle_guild_cmd(
    ident: u16,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    guilds: &mut mir2_world::GuildManager,
    cfg: &mir2_world::GuildConfig,
    sessions: &mut HashMap<String, SessionGround>,
    saves: &HashMap<String, SavedHuman>,
    gates: &HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    ref_range: i32,
) -> Vec<GuildServerEvent>;
```

- `guilds` 放在 `sim_loop` 的局部 `let mut guilds = GuildManager::new();`（G2 空表；G3 启动加载）。不塞进 `World`，避免与 `crates/world` 的 tick 写集纠缠。
- 回帧统一走 `route_to_gate(gates, entry.gate_idx, entry.socket, "...")`；42 的收件人集合用 `chat::hear_targets(world, target_id, ref_range)`（含自己，已处理 `ob_mode/fixed_hide_mode`；500ms 缓存未建模，沿用既有登记）。

---

## 2. G2：会话态字段的归属、写入点与登录时序

### 2.1 归属：`SessionGround`，不是 `Entity`

在 `SessionGround`（`main.rs:11989`）内嵌：

```rust
#[derive(Clone, Debug)]
struct GuildSession {
    guild_name: Option<String>, // PlayObject.MyGuild（manager 是真源，这里是缓存）
    rank_no: i16,               // GuildRankNo（0 = 无）
    rank_name: String,          // GuildRankName
    allow_guild: bool,          // AllowGuild，ctor=false，不持久化
    ban_guild_chat: bool,       // BanGuildChat，ctor=true（名字反向：true=可收行会消息）
}
```

- 默认值：`guild_name=None, rank_no=0, rank_name="", allow_guild=false, ban_guild_chat=true`（oracle `PlayObject.cs:903/1007`）。
- **不要**在 `Entity` 上加行会字段。`MyGuild/GuildRankNo` 是 `PlayObject`（每连接）字段；同角色不会有两个会话（顶号先踢旧）。`Entity` 侧的 `HAM_GUILD`/`InGuildWarArea`/名字色是 G5b，另开批。
- 会话生命周期 = `SessionGround`：进世界建、`GateEvent::Closed`/`drain_kicks` 删。

### 2.2 写入点（全部）

| 字段 | 写入点 | 值/来源 |
|---|---|---|
| `guild_name` | 登录物化 | `guilds.member_of_guild(chr)`（`OrdinalIgnoreCase` 的 `IsMember` 是 `==`，照搬）→ 会名 |
| `rank_no/rank_name` | 登录物化 | 同会 `get_rank_name(chr)`（`OrdinalIgnoreCase`）→ `(rank_no, rank_name)`；找不到则 0/空 |
| `guild_name` | 1038 成功（目标会话） | 掌门会名 |
| `rank_no/rank_name` | 1038 成功（目标会话） | `get_rank_name(target)`（刚加入，必命中；通常是 99/行会成员） |
| `guild_name/rank_no/rank_name` | 1039 删他人成功（目标会话） | `None/0/""`（对应 `MyGuild=null; RefRankInfo(0,"")`） |
| 同上 | 1039 自删解散成功（自己会话） | `None/0/""` |
| `allow_guild` | **无生产写入点** | ctor false；`@Letguild` 未接。测试夹具可置 true 以覆盖成功路径 |
| `ban_guild_chat` | **无生产写入点** | ctor true；GM/SwitchData 未移植；G3 的 `SM_GUILDMESSAGE(104)` 消费它 |

- 1039 删他人时目标可能离线：`GetPlayObject==null` ⇒ 不写会话（没有会话）、不产 750/42；管理器仍已删人。
- 1041 成功后**不清**任何人的 `rank_no/rank_name`（N3 只清管理器 ranks）——这是 oracle 的真实状态，别顺手同步。
- 断线不清会话字段本身，随 `ground_sessions.remove` 一起消失；G3 只需在此之前对管理器调 `DelHumanObj`。

### 2.3 与登录 750 的时序

- oracle：`MyGuild = MemberOfGuild(ChrName)` → 有会才 `GetRankName`（`SendMsg(RM_CHANGEGUILDNAME)` 入队，优先级 `(byte)10301 = 61`）→ `GuildRankNo = rankNo` → `RefShowName()` → 后续 SysMsg（`PlayObject.cs:1391-1405`）。
- Rust：
  1. 在 `inputs` 构造处（`main.rs:15220`）之前/同时，用 `guilds` 物化 `(GuildSession, Option<Vec<u8>>)`；
  2. 有会 ⇒ `inputs.guild_changename = Some(guild_frames::sm_guild_changename_750(Some(&g), &rank_name))`；无会 ⇒ `None`；
  3. `SessionGround` 插入（:15477）时把同一份 `GuildSession` 写进新字段；
  4. `inputs.frames()`（:15654）负责把 750 放到队列的出队位（§3）。
- 因为 750 在队列里，它一定晚于 `658`/`212` 与优先级 0/1 的帧、早于 `654/657/42`；这与「先物化会话再出帧」不冲突。

### 2.4 42（RefShowName）的载荷

- `sm_username(target_id, name_color, show_name)`；`show_name` 来自 `enter::show_name`。
- 现 `enter::show_name` 把 `%guildname` 恒替换为空（`enter.rs:269/336`）。**部署 `Server.conf` 的 `ShowGuildName=0`**，且城堡/攻城分支属 G5b ⇒ G2 生产载荷与现状一致。
- 若要 1:1 覆盖 `ShowGuildName=1`：G2 需给 `ShowNameInputs` 加 `guild_name/rank_name/show_guild_name` 并在 `%guildname` 用 `NoCastleGuildName`（缺省 `%guildname[%rankname]`，`MessageSettings.cs:229`）；城堡分支（`CastleGuildName`）留给 G5b。**建议**：G2 先按部署值（false）判 42 的 ident/ActorId/tail 非空，把 `ShowGuildName=1` 标 `not_verified`，不扩大写集。

---

## 3. G2：750 登录出队序（46 之后、654 之前）的验证方法

### 3.1 模型改动（最小）

`smframe.rs`：

- `EnterWorldInputs` 加 `pub guild_changename: Option<Vec<u8>>`（**已编码帧**，避免 lib 依赖 bin 的 `guild_frames`；`smframe` 属 `game_svr` lib，`guild_frames` 只在 `main.rs` 声明）。
- `LoginQueued` 加变体 `ChangeGuildName`（保持 `Copy`，不带 payload）；常量 `PRIORITY_RM_CHANGEGUILDNAME = (messages::RM_CHANGEGUILDNAME & 0xFF) as u8`（=61）。
- 入队位置：六条 `SendPriorityMsg` 之后、`Username`（⑥，:1455）之前；**仅 `self.guild_changename.is_some()` 时入队**（无会不占堆位）。
- 出队：`LoginQueued::ChangeGuildName => out.push(body.clone())`。
- 该改动在「无会」路径上**不得改变任何字节**；有会路径才多一个 61 优先级元素，堆形状变化由窗口判据兜底。

### 3.2 构造级验证（G2，L）

`smframe::tests::guild_login_750_between_46_and_654`：

1. `inputs.guild_changename = Some(sm 750 帧)`；`let ids: Vec<u16> = frames().iter().map(parse ident)...`；
2. 断言 `pos(46) < pos(750) < pos(654)`，且 46 与 750、750 与 654 之间无其它**可观测帧**（`Turn` 不产帧，允许占位）；
3. 750 的尾逐字节 = `EncodeString("会名/封号")`；
4. 另一例 `None` ⇒ `!ids.contains(&750)`，且整串帧与改动前 golden 逐字节相等（回归）。
- 牙：把优先级写成 1（⇒ 750 跑到 46 前）、写成 152（⇒ 跑到 654 后）、或在 `None` 时也入队 ⇒ 红。

### 3.3 真机窗口（R，放 G3）

- 用现有 `mir2-rs/tests/parity/m3/run_pair.ps1` 两相 pin + 同一会话抓包；
- 已入会角色：断言 750 恰一条、seq 在 46 与 654 之间、尾解码 = `会名/封号`；
- 无会角色：断言 750 **0 条**；
- 牙：把 750 插到 52 前（优先级写成 0）⇒ 序红；无会也发 ⇒ 多帧红。
- tier=R，**不在 G2 判**；G2 的 `review` 写 `window_required` 只在声称已证出队序时才需要。

---

## 4. G2 构造级判据与牙（每条都走生产函数，不复制第二套实现）

夹具复用：`guild_castle_diff.py`（selftest 36/36）+ `guild_castle_scenario.json`；测试 rig 用假 `gates` + 两个 `SessionGround`（会长/成员）+ 空 `World` 上放两个 `Race=Play` 实体。

| # | 用例 | 断言 | 牙（改坏必红） |
|---|---|---|---|
| T1 | `g2_dispatch_9cm_matrix` | 逐条驱动 9 个 ident，断言 `(ident,recog,param,tag,series,tail)` 与 §1.2 表一致；1035≡1036；1042/1043 不进行会臂 | 删任一臂；把 1042 加进 `matches!` |
| T2 | `g2_open_dlg_panel_exact` | 2 公告+1 战表+0 盟友，rank=1 与 99 各一例；753 完整字节含 `会名\r \r1\r<Notice>\r...`、`<KillGuilds>`、`WarGuild { Guild = M2Server.Guild.GuildInfo, WarTick = .., WarTime = .. }`；无会 754 纯头 | `\r \r`→`\r\r`；Series 1→0；战表只打会名；无会回 753 |
| T3 | `g2_member_list_empty_gate` | 无会 ⇒ 0 帧；有会但 ranks 空 ⇒ 0 帧；有成员 ⇒ 756 头 `0,0,0,1` + 文本；>5000 UTF-16 截断沿用 G1 | 无会回 756；空文本仍发 756；用 `chars().count()` |
| T4 | `g2_add_member_codes_and_order` | 1 非掌门 / 2 离线 / 2 没面对 / 3 已在本会 / 4 别会 / 4 rank=400 / **5 allow=false**；成功 757 + rank 99 + `SS_207` + 目标会话 + 目标 `750(会名/行会成员)→42→100(Green)`；失败零状态变化 | `AllowGuild` 检查挪到 `add_member` 后；rank 门改成成员数；目标 750 加尾/漏 42 |
| T5 | `g2_del_member_and_self_disband` | 删他人：759 + 目标 `750 纯头→42` + 会话清空；自删单人会：759 **先于** `750 纯头→42→100(Red)`、会表少一会、`SS_206`；自删多人会 ⇒ 760/3；非成员 ⇒ 760/2；非掌门 ⇒ 760/1 | 删掉 `DelGuild`；自删发 760/4；目标 750 带尾；把 100 写成 Green |
| T6 | `g2_notice_segments` | `"a\rb\r"`→`["a","b"]`；`"a\r\rb"`→`["a","b"]`；`"\r"`→`[""]`；`""`→`[]`；成功重开 753；非掌门/无会 0 帧 | 朴素 `split('\r')`（保留空段）；守卫失败回 754 |
| T7 | `g2_update_rank_n3_zero_frames` | 成功：ranks 清空、**0 帧**、`SS_207`；`-1` 0 帧；`-2..-7` → 761 对应 `Recog`；会话 rank 不变 | 期待 756；「修好」N3 改成 `ranks=parsed`；`-1` 发 761 |
| T8 | `g2_ally_break_unreachable` | 双方掌门面对面 ⇒ 769/-4；无会/没回看 ⇒ 769/-1；断盟 ⇒ 769/0；**768/770/771 任何输入都不出现**；非掌门断盟 0 帧 | `EnableAuthAlly` 默认 true；断盟失败改 771 |
| T9 | `g2_login_750_model` | §3.2 的构造级断言 + 无会回归逐字节 | 优先级 1/152；None 也入队 |
| T10 | `g2_session_ownership` | `GuildSession` 只在 `SessionGround`；默认 `allow=false, ban_chat=true`；登录物化 rank=1/99 两例；`Entity` 无行会字段 | 默认 allow=true；从 Entity 读 guild |
| T11 | `g2_refshowname_42` | `hear_targets` 收件人（含自己）收到 42；`Recog=target actor_id`、尾 = `enter::show_name`（部署 `ShowGuildName=0` 时 `%guildname` 为空）；`ShowGuildName=1` 标 `not_verified` | 用掌门 actor_id；尾带空串/前缀 |

---

## 5. G3（只列写集与顺序，不展开）

**写集**

1. `mir2-rs/crates/world/src/guild.rs`：`GuildManager::run` 返回 `GuildRunOutcome { ended: Vec<GuildWarEnded>, save: Vec<String> }`（把 `check_save_guild_file` 的 bool 收上来）；改 `guild.rs:1423-1442` 既有单测。
2. `mir2-rs/bins/game-svr/src/guild_cmd.rs`：1038/1039/1040/1041 成功路径的 `store.save_guild`；自删的 `save_guild` + `save_guild_list`；保存调用点与 oracle 同序（先状态变换/落盘，再 `SS_*`/回帧）。
3. `mir2-rs/bins/game-svr/src/main.rs`：`EnterConfig` 加 `GuildCastleConfig`/`GuildStore`；`sim_loop` 起世界、`world.clock_ms = origin`（:13890）之后加载名单；10s 门（放在 `process_switch_data` :16244 之后、诊断之前）；`GateEvent::Closed` 在组队退组块之后、`world.disconnect` 之前调 `DelHumanObj`；`drain_kicks`（:11764）同样在 `world.disconnect` 前调；各变更点落盘。
4. `mir2-rs/tests/parity/m3/guild_castle_scenario.json`：只改 `guild_ally_ops.note` 的 `771→769/0`（需 owner 同意，触夹具）。

**顺序**

`(1) 启动加载`（名单 → 逐会 `load_guild_conf`/`load_guild`，缺 `.txt` 的会移除并回写名单；`MissingListFile` 只记日志不拒启）→ `(2) 登录物化`（用已加载 manager）→ `(3) 9 CM 派发 + 即时落盘`（`AddMember/DelMember/UpdateNotice/UpdateRank/Ally/BreakAlly`；`UpdateNotice` 直接 `save_guild` 不置 `BoChanged`，其余走 `update_guild_file` 语义）→ `(4) 10s tick`（`run` → 逐 `ended` 发 `104`、逐 `save` 调 `store.save_guild`）→ `(5) 断线`（`DelHumanObj` 先 `CheckSaveGuildFile`，再按 `Compare(...)==1` 的怪行为清成员；之后删会话）。
- 必须设 `MIR2_CLOCK_MS` 为墙钟量级；否则加载后 `bo_changed=true, dw_save_tick=0` 的首次 30s 门与 C# 分叉（设计件 §3.4 的真实差异）。
- G3 tier=R；G2 先合、G3 后合（同触 `main.rs`/`guild.rs`）。G2 tier=L（纯状态/帧，无 RNG/时序/落库/数值）。

### 5.1 G2 写集（供排串行）

- 新文件 `mir2-rs/bins/game-svr/src/guild_cmd.rs`（9 handler + rig 单测）。
- `mir2-rs/bins/game-svr/src/main.rs`：`mod guild_cmd`；`SessionGround` 加 `guild: GuildSession` 并初始化；dispatch 单臂；登录物化 + `inputs.guild_changename`；`EnterConfig.guild_cfg` + 初始化。
- `mir2-rs/bins/game-svr/src/smframe.rs`：`EnterWorldInputs.guild_changename`、`LoginQueued::ChangeGuildName`、优先级常量、入队/出队。
- `mir2-rs/crates/world/src/guild.rs`：`open_dlg_text`、`notices_from_text`（复用 `mir2_shared::hutil32::get_valid_str3`），纯函数无 IO。
- `mir2-rs/bins/game-svr/src/enter.rs`（可选，仅当做 `ShowGuildName=1`）：Guild 后缀；否则登记 `not_verified`。

---

## 6. 复核命令（只读，已在本 worktree 跑过）

```bash
grep -n "CM_GUILD" mir2-rs/bins/game-svr/src/main.rs            # 无输出（G2 前）
grep -n "CM_OPENGUILDDLG\|CM_GUILDHOME\|CM_GUILDBREAKALLY" mir2-rs/crates/protocol/src/messages.rs
grep -n "fn frames(&self)\|enum LoginQueued" mir2-rs/bins/game-svr/src/smframe.rs
grep -n "ground_sessions.remove(&chr_name)\|world.disconnect" mir2-rs/bins/game-svr/src/main.rs
grep -n "SendSocket(CommandMessage defMsg" -A 15 src/M2Server/Player/PlayObject.Socket.cs
sed -n '967,995p' src/M2Server/Guild/GuildInfo.cs                # N3 清空
sed -n '1550,1556p' src/M2Server/Player/PlayObject.Operate.cs   # 769/0
```

## 7. 残留物

- 仅本文件 `E:/tmp/laneB-guild-g2.md`；无 worktree、无 `CARGO_TARGET_DIR`、无编译产物、无进程、未写仓库。

## 8. `GuildConfig` 的构造（G2 不要临时用 default）

- 生产值必须来自同一份 `Server.conf`：`mir2_data::guild_castle::GuildCastleConfig::load(&conf)?`（`guild_castle.rs:301`，内部按 BOM 探测编码；`conf` = 主程序已算出的 `Server.conf` 路径），再映射到 `mir2_world::GuildConfig`：
  - `guild_war_time ← cfg.guild_war_time`
  - `guild_member_max_limit ← cfg.guild_member_max_limit`
  - `guild_rank_name_len ← cfg.guild_rank_name_len`
  - `guild_name_len ← cfg.guild_name_len`
  - `guild_chief`/`guild_member_rank` ← `[String]`（缺键用 `GameSvrConf` 初值 `掌门人`/`行会成员`）
  - `guild_msg_f_color`/`guild_msg_b_color`/`guild_recall_time`：Rust 配置层**尚未解析**（`GateConfig` 与 `GuildCastleConfig` 都没有这三键）⇒ G2 先用 `GuildConfig::default()` 的 0xDB/0xFF/180，并在卡上登记；G3 若要判 `SM_GUILDMESSAGE(104)` 的颜色/召回，必须补 `[Setup]` 三键解析（源 `ServerConf.cs:335/336/479`）
- 建议 G2 就把 `GuildCastleConfig` 塞进 `EnterConfig`（G3 直接复用建 `GuildStore`），避免 G2 用 `GuildConfig::default()` 蒙混过关；`GuildFileSections::from_config(&guild_castle_cfg)` 在 G3 才用。


---

## 评审员更正（2026-10-20，**以本节为准**）

1. **夹具 update_rank 的预期错误**：`guild_castle_scenario.json` 的载荷 `#99 成员` 首个 rank 的 `RankNo != 1`
   ⇒ `UpdateRank` 返回 -2（`GuildInfo.cs:827-845`），发 `761/-2`；随后 `del_member peer` 是 `759`，**不是 760/2**。
   要验证 N3 必须让 update_rank 返回 0（首个 rank 为 `#1 <封号>` 且成员表通过 -5/-6 检查）。
2. **T9 的两颗牙不咬**：在正确入队位（六条之后、Username 之前）插入 p=61 时，写 1 或 152 都不改变出队序；
   正确牙是 **0**（750 到 46 前）或 **>=154**（750 到 654 后）。且「46 与 750 之间无其它可观测帧」不成立——
   生产 `adjust_bonus=Some` 时 811 夹在中间，插入 750 还会把 811/46 的相对序翻过来（oracle 的堆形）。
3. **tier**：750 入队改的是 `smframe` 的出队序 ⇒ 时序面 **R**。拆成 **G2a（9 CM + 会话态 + 帧，L）** 与
   **G2b（750 入队/出队序，R，占窗口）**，或把 750 整体挪进 G3（R）。
4. **写集**：PL-1 早已合并；真实冲突是 **PL-4**（`SessionGround`/`sim_loop`/`crates/world`）。G2 必须排在 PL-4 之后
   （现已满足：PL-4 于 `bd5dc1bc3` 合并）。
