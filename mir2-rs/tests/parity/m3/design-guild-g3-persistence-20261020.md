# Lane B — 行会 G3（持久化 + 10s/30s + 掉线）派工简报

- 基线：`b0e618c33`（master，2026-10-20）。本车道**只读**：不建 worktree、不改仓库、不起栈、不碰 `E:/MirServer`，
  也不碰 `/e/tmp/wg2`、`/e/tmp/wslots` 两条车道（`game-svr` 与 login/sel-gate 的写集）。
- 输入：`mir2-rs/tests/parity/m3/design-guild-session-layer-20261019.md`（§0/§3/§4）、
  `mir2-rs/tests/parity/m3/design-guild-g2-dispatch-20261020.md`（§5 + 评审员更正）、
  oracle `GuildManager.cs:78-167` / `GuildInfo.cs:195-346,366-431,525-550,584-621` /
  `SystemProcessor.cs:70-77` / `PlayObject.Message.cs:2129-2150` / `PlayObject.cs:3422-3530`。
- 已落地：G1 `bins/game-svr/src/guild_frames.rs`（640 行，15 帧）、`crates/data/src/guild.rs` 的 `GuildStore`、
  `crates/world/src/guild.rs` 的 `GuildManager`。**G2 尚未合入**（`bins/game-svr/src/guild_cmd.rs` 不存在、
  `main.rs` 无 `CM_GUILD` 臂、`SessionGround` 无 `GuildSession`）⇒ G3 依赖 G2 先合。
- 本文件行号为 `b0e618c33` 实测；G2 合入后 `main.rs` 行号整体位移，锚点以函数/结构名为准。

---

## 0. 开工前先钉死的 8 条口径（与既有设计件不一致处，以本节为准）

1. **四段段头与两个封号名来自 `String.conf` 的 `[Guild]`，不是 `Server.conf`**
   （`StringConf.cs:30-35`：`GuildNotice/GuildWar/GuildAll/GuildMember/GuildMemberRank/GuildChief`；
   `GameSvrConf.cs:1850-1855` 只是缺省值）。`GuildCastleConfig` 里同名六字段是**「无读取点」**
   （`crates/data/src/guild_castle.rs:66`）⇒ **不要**用 `GuildFileSections::from_config(&GuildCastleConfig)`，
   否则 String.conf 一旦覆盖就静默分叉。当前 `bins/game-svr/src/strings.rs:13` 明确「只解析 `[String]` 段」
   ⇒ **G3 必须补 `[Guild]` 六键**（写集 4）。`GuildMsgPreFix`（缺省 `〖行会〗`）也缺，见 §2.6。
2. **行会成员 → `PlayObject` 的绑定不是在线表**（G3 的关键发现）。
   `GuildMember.PlayObject` 只在三个点被赋值：`LoadGuildFile`（`GuildInfo.cs:337`，启动时无人上线 ⇒ **恒 null**）、
   `SetGuildInfo`（`:477`）、`AddMember`（`:647`）；`GetRankName` 里的回填是**注释掉的**（`:495`），
   `UpdateRank` 里的回填（`:776`）随后被 N3 清表。⇒ **重启后，从磁盘加载的成员永远没有绑定**，
   `SendGuildMsg`（`:431-449`）与 `RefMemberName`（`:350-364`）对它们**一个帧都不发**，即使本人已登录。
   G2 的 `ref_member_name(online)`（`crates/world/src/guild.rs:617-627`）把「绑定」等同于「当前在线」，
   口径偏了；G3 必须把它改成**按成员绑定**（推荐 `GuildMemberRef { name, actor_id: Option<i32> }`）。
3. **`UpdateGuildFile` 是「立即落盘 + 标记」**（`GuildInfo.cs:584-589`）⇒ `Run` 里战争到期必须**当场**发 save 事件；
   `CheckSaveGuildFile` 不会兜底（`DwSaveTick` 刚被置成 now，30s 差为 0）。
4. **`DelHumanObj` 的 `CheckSaveGuildFile` 在清成员之前**（`:534-537`）⇒ 宿主必须先落盘、再改成员，
   否则写出的文件少了本该还在的成员（§3.3 的两步拆分）。
5. **10s/30s 都是严格 `>`**（`SystemProcessor.cs:72`、`GuildInfo.cs:527`）；Rust 侧 `>`、i32 回绕差。
6. **`MIR2_CLOCK_MS` 必须取「开机经过毫秒」量级的小值（如 `3_600_000`），不能用 epoch 毫秒**。
   `GuildState::dw_save_tick` 的初值是**绝对哨兵 0**，`CheckSaveGuildFile` 直接算 `now_tick - 0`；
   `now_tick = world.clock_ms as i32`，epoch `1_700_000_000_000` 转 i32 后是负数 ⇒ 30s 门**永不触发**。
   （`bins/game-svr/tests/clock_inject.rs:24` 的 `1_700_000_000_000` 只验证探针，不可用于行会窗口。）
7. **加载把 `WarTime` 读成 0**（`s20` 跨行残留，`GuildInfo.cs:212/288`）⇒ 加载后的**第一次保存会删掉全部「敌对行会」行**
   （`n14 = 0 - (now-war_tick) <= 0` 被跳过）。这是 oracle 的 round-trip 事实，不是 bug，照抄。
8. **`.bak` 永不落盘、新会 `<名>.txt` 写不出去**：`.bak` 路径是 `<GuildDir>/<会名>/.<tick>.bak`
   （`GuildInfo.cs:595`，三段 `Path.Combine`）——`<GuildDir>/<会名>/` 这个**子目录从不创建** ⇒
   `StringList.SaveToFile` 的 `FileMode.Open` 抛异常被 catch 吞掉。新会 `.txt` 同理（`FileMode.Open`）。
   Rust 数据层已照搬（`MissingSaveTarget`），G3 接线时**不要补全**。

---

## 1. 启动加载与逐文件格式

### 1.1 启动顺序与插入点

oracle：`GameApp.InitializeWorld` 在 `WorldEngine.Initialize()` 之后、`CastleMgr.LoadCastleList()` 之前
调 `GuildMgr.LoadGuildInfo()`（`GameApp.cs:244-246`）。

Rust（`main.rs`，G2 合入后按名字定位）：

1. `main()` 里 `base = mapwire::base_path(&conf)`（`:415`）之后：
   ```rust
   let guild_castle = mir2_data::guild_castle::GuildCastleConfig::load(&conf)?; // [Share] GuildDir/GuildFile、[Setup] GuildRankNameLen
   let guild_store = mir2_data::guild::GuildStore::new(
       base.join(&guild_castle.guild_dir), base.join(&guild_castle.guild_file));
   // sections / chief / member_rank：来自 String.conf [Guild]（口径 1），不是 guild_castle
   ```
   把 `guild_store`、`guild_sections`、`guild_chief`、`guild_member_rank` 塞进 `EnterConfig`（`:12266`）。
2. `sim_loop` 里，**时钟注入（`:14090-14099`）之后**（`world.clock_ms` 已是 origin）：
   ```rust
   let mut guilds = mir2_world::GuildManager::new();
   let mut guild_run_tick = world.clock_ms as i32;      // oracle Startup 里 RunTimeTick = GetTickCount()
   if let Some(store) = enter_cfg.guild_store.as_ref() {
       match store.load_guild_info(&enter_cfg.guild_sections, guild_cfg.guild_rank_name_len,
                                   world.clock_ms as i32, i32::from(enter_cfg.server_index)) {
           Ok(load) => { guilds.load_guild_info(&load.guilds);
                         glog!("INF", "已读取 [{}] 个行会信息...（移除 {}）", load.guilds.len(), load.removed.len()); }
           Err(GuildStoreError::MissingListFile(p)) => glog!("ERR", "行会信息文件未找到!!! {p}"),
           Err(e) => glog!("ERR", "行会初始化失败：{e}"),   // 差异化表第 12 行
       }
   }
   ```
3. 失败口径：`MissingListFile` ⇒ 只记日志、会表为空、**不拒启**（`GuildManager.cs:106-109`）。

### 1.2 `GuildBase/GuildList.txt`（`[Share] GuildFile`）

- **写**（`SaveGuildList`，`GuildManager.cs:124-143`）：
  - `ServerIndex != 0` ⇒ 直接 `return`（不写）；Rust `SaveStatus::SkippedNonZeroServerIndex` 已对。
  - 编码：`StringList.SaveToFile` 先 `GetEncoding(fileName, gb2312)`（`StringList.cs:293-343`）：
    **`FileMode.Open`**（文件不存在 ⇒ 抛异常，被 `try/catch` 吞掉并记 `行会信息保存失败!!!`）；
    前 4 字节 BOM 嗅探：`FE FF` ⇒ UTF-16BE、`FF FE` 且第 3 字节 ≠ `FF` ⇒ UTF-16LE、
    `EF BB BF` ⇒ UTF-8，否则 **GB2312**。`new StreamWriter(fileName, false, fs)` 覆盖写。
  - 行序 = `GuildList` 顺序，每行一会名，`WriteLine` ⇒ **每行都以 CRLF 结尾（含最后一行）**。
  - 例（GBK 字节）：`会甲\r\n会乙\r\n`。
- **读**（`LoadGuildInfo`，`:81-93`）：同一 BOM 嗅探；`ReadLine()` 逐行（`\r\n`/`\n`/`\r` 三种终止）；
  每行 `Trim()`，空行跳过。随后**先为所有名字 `new GuildInfo`（建 `.ini`），再逆序 `LoadGuild`**。

### 1.3 `GuildBase/Guilds/<名>.txt`（`SaveGuildInfoFile` 的 0 区路径）

- 写：`SaveGuildFile`（`GuildInfo.cs:384-431`）。编码/CRLF 规则同 1.2（同一 `StringList.SaveToFile`）。
- **行序固定**：

  ```
  公告\r\n
  +<NoticeList[0]>\r\n
  +<NoticeList[1]>\r\n
   \r\n                                   ← 单空格分隔行
  敌对行会\r\n
  +<敌会名> <n14>\r\n                    ← n14 = WarTime - (now - WarTick)，n14 <= 0 整条跳过
   \r\n
  联盟行会\r\n
  +<盟友名>\r\n                          ← GuildAllList 恒空 ⇒ 生产只有分隔行
   \r\n
  行会成员\r\n
  #<RankNo> <RankName>\r\n               ← RankNo 十进制无补零
  +<成员名>\r\n
  ...
  ```

  每个 `+` / `#` 前缀都是字面量；最后一行也带 CRLF。
- 空会（`BackupGuildFile` 之后）逐字节 =
  `公告\r\n \r\n敌对行会\r\n \r\n联盟行会\r\n \r\n行会成员\r\n`（7 行，GBK）。
- `n14` 是 **int 先算后拓宽 long**：`WarTime - (GetTickCount() - WarTick)`，Rust 用
  `i64::from(war_time.wrapping_sub(now.wrapping_sub(war_tick)))`（已对，`data/guild.rs:597-620`）。
- 加载：`LoadGuildFile`（`:209-346`），`GetValidStr3` 分隔符 = `{' ', ','}`；`;` 行与空行跳过；
  段头是**整行相等**；`#` 行重置当前 rank；`+` 行进对应段。`WarTime` 读 `s20`（恒空）⇒ 0。

### 1.4 `GuildBase/Guilds/<名>.ini`

- 路径：`<GuildDir>/<名>.ini`（`GuildInfo.cs:127`）。
- **只可能是一个 0 字节文件**：`GuildConf` ctor 对不存在的文件先 `WriteString`（只写内存 cache），
  再 `Load()`；`Load()` 见文件不存在 ⇒ `File.Create(...).Close()`（`ConfigFile.cs:256-261`）。
- `SaveGuildConfig`（`GuildConf.cs:25-33`）只 `WriteString/WriteInteger`，**全仓无 `Save()` 调用**
  ⇒ 五键 `BuildPoint/Aurae/Stability/Flourishing/ChiefItemCount` **永不落盘**。
- 读侧：`[Guild]` 段、GB2312（BOM 嗅探）、`;` 行与 `/* */` 跳过、**段重复直接结束整个加载**
  （`ConfigFile.cs:302-305` 的 `goto Label_02AE`；Rust `IniFile` 已照搬 `break`）。
- 副作用是**全局顺序**的：`LoadGuildInfo` 对名单里**每一个**名字先建 `.ini`，再逐个解析 `.txt`。

### 1.5 `.bak`（`BackupGuildFile`，`GuildInfo.cs:591-621`）

- 先尝试写 `<GuildDir>/<会名>/.<tick>.bak`（三段 `Path.Combine`；子目录不存在 ⇒ 抛异常被吞）；
- 然后清 `MemberList`（`= null`）、`RankList`、`NoticeList`、`GuildWarList`、`GuildAllList`；
- 最后 `SaveGuildInfoFile()` ⇒ 用**空段体**覆盖 `<名>.txt`（见 1.3 的空会字节）；
- 不置 `BoChanged/DwSaveTick`（`GuildState::backup_guild_file` 已对，`world/guild.rs:526-537`）。

### 1.6 与 Rust `GuildStore` 的差异

| # | oracle | Rust 现状 | G3 处理 |
|---|---|---|---|
| 1 | 段头/封号名来自 `String.conf [Guild]` | `GuildFileSections::from_config(GuildCastleConfig)`；该结构六字段「无读取点」 | **必须改**：解析 `[Guild]` 六键（写集 4） |
| 2 | 保存时按 BOM 嗅探切换编码（UTF-8/UTF-16 文件会被原编码重写） | `write_existing` 恒 GBK | 登记（T-9x）：部署件均无 BOM；BOM 输入会分叉 |
| 3 | 新会 `.txt` 打不开 ⇒ 吞异常、**不创建** | `MissingSaveTarget`、不创建 | 一致 |
| 4 | `.ini` 只建空、从不写 | 同 | 一致 |
| 5 | `.bak` 永不落盘 | `backup_guild_file` 只清状态、无写路径 | 一致；**不要**建子目录 |
| 6 | 缺 `.txt` 的会：逆序移除 + 回写名单（`guild_exists` 查**当前存活集**） | 已对（`load_guild_info`） | 一致（已有 `guild_store_vectors.rs` 覆盖） |
| 7 | `WarTime` 加载恒 0、`WarTick=now` | 已对 | 一致 |
| 8 | 非 0 区保存路径 `Path.Combine(dir,name,".",idx)` → `<dir>/<name>/<idx>` | `.join(format!(".{idx}"))` → `<name>/.<idx>` | 不可观测（父目录不存在，两者都 `MissingSaveTarget`）；**可选**小修 |
| 9 | 成员持有 `PlayObject` 绑定 | 只有 `Vec<String>` | **必须补**（口径 2） |
| 10 | `Run` 的 `CheckSaveGuildFile` 立即落盘 | `run` 丢弃 bool（`world/guild.rs:954`） | **必须改返回类型**（§2.3） |
| 11 | `DelHumanObj` 先 CheckSave 再改 | `del_human_obj` 内部调用后丢 bool（`:494-506`） | **必须拆两步**（§3.3） |
| 12 | `.ini` 创建失败会抛到 `InitializeWorld` 的 catch（初始化中断） | `?` 传播，由宿主决定 | 建议：记 ERR、会表为空（fail-closed），登记 T-9x |
| 13 | 职名截断是 `s24.Length > 30` 的 **UTF-16 码元** | `chars().count()` / `chars().take()` | GBK 解码只出 BMP ⇒ 等价；换非 GBK 输入才会分叉（登记一句） |
| 14 | `ConfigFile.Load` 的 `ReadLine` 认孤立 `\r` | `IniFile` 用 `text.lines()`（只认 `\n`） | 部署 `.ini` 为 0 字节 ⇒ 不可观测；登记 |
| 15 | 加载后 `bo_changed = true`（五个 property setter），`dw_save_tick` 仍 0 | `set_conf` 已对 | 一致（这是 30s 门首跳的来源） |

---

## 2. 10s 门 / 30s 门 / 变更点落盘

### 2.1 oracle 时序

- `SystemProcessor` 每 **50ms** 一轮（`:12`）；`ProcessGameRun`：`if (now - RunTimeTick) > 10000 { RunTimeTick = now;
  GuildMgr.Run(); CastleMgr.Run(); ... }`（`:70-77`）。`RunTimeTick` 在 `Startup` 置 `GetTickCount()`（`:24`）。
- `GuildMgr.Run`（`GuildManager.cs:145-167`）逐会：
  1. 倒序扫 `GuildWarList`：`(now - WarTick) > WarTime` ⇒ `EndGuildWar`（SendGuildMsg）+ `RemoveAt` + `boChanged=true`；
  2. `if (boChanged) guild.UpdateGuildFile();`（**立即 SaveGuildInfoFile**）；
  3. `guild.CheckSaveGuildFile();`（`bo_changed && >30000` ⇒ `bo_changed=false` + SaveGuildInfoFile）。

### 2.2 Rust 插入点

`sim_loop` 每 **200ms** 一 tick（`mir2_world::TICK_INTERVAL_MS`）。插入位置：
`process_switch_data(...)`（`:16480`）之后、`if ticks.is_multiple_of(50)` 诊断（`:16499`）之前：

```rust
let now_i32 = world.clock_ms as i32;                 // oracle int；回绕语义与 T-90 同族
if now_i32.wrapping_sub(guild_run_tick) > 10_000 {
    guild_run_tick = now_i32;
    for ev in guilds.run(now_i32) {
        match ev {
            GuildRunEvent::WarEnded { guild, message, .. } => {
                // SendGuildMsg：只发给 ranks/members 中 actor_id.is_some() 的成员（口径 2）
                for (name, actor) in guilds.send_guild_msg_targets(&guild) { /* 104 */ }
            }
            GuildRunEvent::Save { guild, .. } => save_one(&store, &guilds, &guild, /*…*/),
        }
    }
}
```

- `guild_run_tick` 初值 = **加载时**的 `world.clock_ms as i32`（§1.1），不是 0。
- **不要在 `tick_head`/`tick_tail` 之间插**：oracle 的 10s 块在 `WorldEngine.Run` 之后，Rust 放在本 tick 全部下行之后
  可保证帧面不夹进 P1..P8 段。
- 200ms 量化 ⇒ 首跳在 +10200（oracle +10050），30s 边界最多晚 150ms（T-9x）。

### 2.3 `run` 的返回类型（必改）

```rust
pub enum GuildRunEvent {
    WarEnded { guild: String, enemy: String, message: String },
    Save { guild: String, reason: GuildSaveReason },
}
pub enum GuildSaveReason { UpdateGuildFile, CheckSaveGuildFile }
pub fn run(&mut self, now_tick: i32) -> Vec<GuildRunEvent>
```

每会事件序 = oracle 语句序：`WarEnded*`（倒序 j）→ `Save{UpdateGuildFile}`（若有到期）→
`Save{CheckSaveGuildFile}`（若 30s 命中）。**两者不会同时**（`UpdateGuildFile` 把 `dw_save_tick` 置 now）。
同步改既有单测 `manager_run_ends_expired_wars`（`:1423`）与 `request_then_run_expires_war_n2`（`:1820`）。

### 2.4 每个变更点的「立即落盘 + 标记」

| # | 变更点 | oracle 落盘（按语句序） | Rust 接线 |
|---|---|---|---|
| 1 | `AddGuild`（G4 NPC） | ctor 建 `.ini`；`SetGuildInfo` → `SaveGuildInfoFile`（txt 不存在 ⇒ 吞）；`Add`；`SaveGuildList` | `load_guild_conf(name)` → `add_guild(...)` → `save_guild`（忽略 `MissingSaveTarget`）→ `save_guild_list(names)` |
| 2 | `DelGuild` | `BackupGuildFile`（.bak 吞 + 清 + 写空 txt）→ `RemoveAt` → `SaveGuildList` | 先取 `to_record()`（清空后的体）→ 移除 → `save_guild` → `save_guild_list` |
| 3 | 1038 成功 | `AddMember` 内 `UpdateGuildFile`（置 `bo_changed=true`、`dw_save_tick=now`、落盘） | `add_member(..., actor_id)` 之后**立即** `save_guild`；再 `SS_207` → 目标 750/42/100 → 757 |
| 4 | 1039 删他人 | `DelMember` 内 `UpdateGuildFile` | `del_member` 返回 true ⇒ `save_guild`；再目标 750/42 → `SS_207` → 759 |
| 5 | 1039 自删解散 | `CancelGuld` → `BackupGuildFile`（写空 txt）；`DelGuild` → `BackupGuildFile`（**再写一次空 txt**）+ 移除 + `SaveGuildList` | `cancel_guld` ⇒ save 空体；`del_guild` ⇒ save 空体（用 cancel 后的快照即可，字节相同）；再 `save_guild_list` |
| 6 | 1040 公告 | 重建 `NoticeList` → `SaveGuildInfoFile`（**不置 BoChanged**）→ `SS_207` → 重开 753 | 只 `save_guild`，**绝不**调 `update_guild_file` |
| 7 | 1041 成功 | `UpdateRank` 尾 `UpdateGuildFile`（N3 已清 ranks） | `update_rank` 返回 0 ⇒ `save_guild`（此时 ranks 空） |
| 8 | 1044/1045 成功 | `SaveGuildInfoFile`（`AllyGuild`/`DelAllyGuild`） | 生产不可达（769/-4、769/0）；留 save 钩子即可 |
| 9 | 宣战 | 双向 `AddWarGuild` 内 `UpdateGuildFile` | G4 接 NPC 时对**两**个会各 `save_guild` |
| 10 | `Run` 战争到期 | `UpdateGuildFile` | `GuildRunEvent::Save{UpdateGuildFile}` |
| 11 | `Run` 30s | `CheckSaveGuildFile` | `GuildRunEvent::Save{CheckSaveGuildFile}` |
| 12 | `DelHumanObj` | 先 `CheckSaveGuildFile` 再清成员 | 先 `check_save_guild_file(now)`（true ⇒ `save_guild`）再 `del_human_obj_member` |
| 13 | 加载 | 五 setter ⇒ `bo_changed=true` | `load_guild_info` 已对；首跳由 §2.5 保证 |

**API 补充**：`GuildState::to_record(&self) -> GuildRecord`（`name + conf() + to_body()`）；
`GuildManager::names(&self) -> Vec<String>`；若要拿到清空后的 record，先 `find_guild_mut` 读快照再 `del_guild`，
或让 `del_guild` 返回 `Option<GuildRecord>`。

### 2.5 时钟域与 `MIR2_CLOCK_MS`（判据可判的前提）

- 所有 `GuildState` 时间戳用 `world.clock_ms as i32`（`world/guild.rs` 模块头的既有口径），**不要混用 `now_ms()`**。
- 未注入时 `world.clock_ms` 从 0 起：加载后 `bo_changed=true, dw_save_tick=0`，`Run` 首跳 10200 时
  `10200 > 30000` 为假 ⇒ **第一次 30s 保存落在 30600**（第三跳），而 C# `Environment.TickCount` 是开机毫秒数
  （>30s）⇒ **首跳 10200 就补写**。⇒ 行会窗口必须注入**小值**（口径 6）。
- 判据：`origin=0` ⇒ 10200/20400 无 Save、30600 首次 Save；`origin=3_600_000` ⇒ 10200 首次 Save、`bo_changed` 复位。

### 2.6 `SM_GUILDMESSAGE(104)` 的扇出（`Run` 唯一出帧点）

- `SendGuildMsg`（`GuildInfo.cs:431-449`）：逐 rank/成员，`PlayObject == null` 跳过；
  `if (guildMember.BanGuildChat)`（**默认 true，名字反向**）才 `SendMsg(guildMember, RM_GUILDMESSAGE, 0, FColor, BColor, 0, sMsg)`。
  ⇒ `Recog` = **收件人自己**的 ActorId；WParam 低字节 F、高字节 B；`Series=1`；尾 `EncodeString(prefix+text)`。
- 前缀：`if (Config.ShowPreFixMsg) sMsg = Config.GuildMsgPreFix + sMsg;`。部署 `ShowPreFixMsg=0`
  （已由 `line_notice_cfg.show_pre_fix_msg` 解析）⇒ 无前缀；`GuildMsgPreFix` 缺省 `〖行会〗`（`String.conf [String]`，**G3 补读**）。
- 颜色 `[Setup] GuildMsgFColor/GuildMsgBColor`（缺省 `0xDB/0xFF`）Rust **尚未解析** ⇒ G3 补两键（写集 5）；
  判据用注入值，不要依赖 `GuildConfig::default()` 蒙混。
- 收件人集合 = **绑定成员**（口径 2），再用 `actor_id` 在 `online` 里找会话；找不到 ⇒ 不发（== oracle 的 ghost 队列不排水）。

---

## 3. 掉线：`DelHumanObj` 与会话销毁顺序

### 3.1 oracle 两条路径

- **真断线**：`WorldServer.ProcessPlayObjectData` 的 `else` 支：`PlayObjectList.Remove` → `playObject.Disappear()`
  → `AddToHumanFreeList` → `DealCancelA` → `SaveHumanRcd` → `CloseUser`（`WorldServer.cs:708-715`）。
  `Disappear()`（`PlayObject.Message.cs:2129-2150`）顺序：
  1. `if (BoReadyRun) DisappearA();`
  2. `Transparent && HideMode` 清透明态；
  3. `if (GroupOwner != 0) groupOwnerPlay.DelMember(this);`（退组）
  4. `if (MyGuild != null) MyGuild.DelHumanObj(this);`（**行会**）
  5. `LogonTimcCost(); base.Disappear();`（基类空实现）
- **顶号**：`BoKickFlag` → `RunNotice`（`PlayObject.cs:1079-1090`）发 `SM_OUTOFCONNECTION` + `MakeGhost()`；
  `MakeGhost` 只做 `Ghost=true` + `DisappearA()`，**不含** `DelHumanObj`。下一次 `ProcessPlayObjectData`
  （`RunTime=250ms`，50ms 扫描）看到 `Ghost` 才走 `Disappear()`（含行会）。⇒ 顶号的 `DelHumanObj` 比踢晚 ≤250ms。

### 3.2 `Compare(...) == 1` 怪口径（`GuildInfo.cs:534-550`）

```
CheckSaveGuildFile();
for rank, for j:
    if (string.Compare(MemberName, playObject.ChrName, OrdinalIgnoreCase) == 1) {
        MemberList[j] = default(GuildMember);   // 不是 RemoveAt
        return;
    }
```

- `== 1` = 「成员名 **严格大于** 断线者名」⇒ **正常断线（同名比较为 0）永远不会删掉自己**；
  实际被清空的是**第一个名字比断线者大的其他成员**；等值/小于都不动。
- `= default(GuildMember)` ⇒ 条目**留在表里**，`MemberName=null`、`PlayObject=null`；
  下次保存写出 `"+" + null` = **一行 `+`**；加载时该行产生空 token ⇒ 被跳过；`IsMember` 跳过空名。
- Rust 已照搬为 `members[j] = String::new()`（`world/guild.rs:494-506`），序列化同为 `+\r\n`。
- 判据必须三向覆盖：`member > name`（清空）、`==`、`<`（不动），并断言「清空后条目仍在、`count()` 不变、
  `is_member("")` 为 false」。

### 3.3 Rust 接线（`GateEvent::Closed`，`main.rs:15015`）

当前顺序（`:15030-15120`）：`engine.delete_human` → `deal_cancel_between` → `online_take` →
`save_human_rcd` → `saves.remove` → `ground_sessions.remove`（`:15063`）→ shop/sell_off 移除 →
`displays.remove` + **party `DelMember` 帧** → `world.disconnect`。

接法（保持 oracle 顺序「退组 → 行会 → 摘世界」）：

1. 在 `ground_sessions.remove` **之前**取
   `let guild_name = ground_sessions.get(&chr_name).and_then(|sg| sg.guild.guild_name.clone());`
2. 在 party 块之后、`world.disconnect` **之前**：
   ```rust
   if let Some(gname) = guild_name.as_deref() {
       if let Some(g) = guilds.find_guild_mut(gname) {
           if g.check_save_guild_file(now_i32) { save_one(/*…*/); }  // 先落盘
           let _ = g.del_human_obj_member(&chr_name);                // 再清（Compare==1）
       }
   }
   ```
3. 断线**不产任何行会帧**（不动 `750`），会话字段随 `ground_sessions.remove` 消失（G2 已定）。
4. `drain_kicks`（`:11806-11851`）同样：`save_human_rcd` 之后、`world.disconnect` 之前插入同两行；
   但 oracle 的顶号路径还有「party `DelMember`」，Rust `drain_kicks` 目前**没有**（既有缺口，别在本卡顺手补）。
5. `now_i32 = world.clock_ms as i32`（事件循环里世界时钟停在上一 tick）。

### 3.4 需要拆的两步（必改）

`GuildState::del_human_obj` 现在「检查 + 清成员」一体，且丢 bool。改成：

```rust
pub fn del_human_obj_member(&mut self, name: &str) -> bool    // 只做 Compare==1 循环
pub fn del_human_obj(&mut self, name: &str, now: i32) -> bool // 保留给纯状态调用方：check + member
```

宿主用两步版，保证「保存的字节里成员还在，清空只影响内存/下一次保存」——这是 oracle 的字节事实。

---

## 4. 写集 / 串行 / 判据

### 4.1 写集（G3）

| # | 文件 | 改动 |
|---|---|---|
| 1 | `mir2-rs/crates/world/src/guild.rs` | `GuildRunEvent`/`GuildSaveReason` + `run` 改签名；`del_human_obj` 拆两步；`GuildMemberRef{name, actor_id}`（或等价绑定表）+ 全部成员的读写点；`to_record`；`GuildManager::names`；`del_guild` 交出清空体；`ref_member_name`/`send_guild_msg_targets` 改用绑定；改单测 `:1266/:1423/:1820` |
| 2 | `mir2-rs/bins/game-svr/src/guild_cmd.rs`（G2 新建） | 每个变更点按 §2.4 调 `save_guild`/`save_guild_list`；`add_member`/`set_guild_info` 传 `actor_id`；104 扇出 |
| 3 | `mir2-rs/bins/game-svr/src/main.rs` | `EnterConfig` 加 `guild_store/guild_sections/guild_chief/guild_member_rank`；`main()` 建 store；`sim_loop` 加载 + `guild_run_tick` + 10s 门；`Closed`/`drain_kicks` 两处；把 `&mut guilds`/`&store` 传给 `handle_guild_cmd` |
| 4 | `mir2-rs/bins/game-svr/src/strings.rs` | 解析 `String.conf [Guild]` 六键 + `[String] GuildMsgPreFix`（缺省见 §0.1） |
| 5 | `mir2-rs/bins/game-svr/src/config.rs` | `[Setup] GuildMsgFColor/GuildMsgBColor`（缺省 `0xDB/0xFF`；`GuildRecallTime` 可同批） |
| 6 | `mir2-rs/tests/parity/whitelist.md` | 新增 T-9x（§4.5） |
| 7（可选） | `mir2-rs/crates/data/src/guild.rs` | 非 0 区保存路径 `.<idx>` → `<idx>`（差异表 8，不可观测） |

**不碰**：`crates/protocol`、`crates/storage`、`src/**`、`sql/**`、`MirClient/`、`E:/MirServer`、
`guild_castle_scenario.json`（G2 已改 note；G3 不加 case）。

### 4.2 串行顺序

```
(在跑的 /e/tmp/wg2 game-svr 车道) → G2（9 CM + 会话态 + 750）→ G3（本卡）
```

- G3 与 G2 同触 `main.rs`/`crates/world/src/guild.rs`/`guild_cmd.rs` ⇒ **G2 先合、G3 后合**，G3 rebase 后重跑四门禁。
- 与 `/e/tmp/wslots`（login/sel-gate）无写集重叠；但两者都动 `bins/game-svr`，合并前 rebase 一次即可。
- 本卡不建 worktree（派工时由 owner 建）；`CARGO_TARGET_DIR` 用车道自己的，收尾删除。

### 4.3 构造级判据（≥8，全部走生产函数 + 影子目录，不复制第二套实现）

| # | 用例 | 断言 | 牙（改坏必红） |
|---|---|---|---|
| T1 | `g3_startup_load_wiring` | 影子目录：`GuildList.txt`={A,B,C}，A/B 有 `.txt`、C 缺；A 的战表引用 C、B 的战表引用 B。加载后：会表=[A,B]（名单序）；C 被移除且 `GuildList.txt` 回写为 `A\r\nB\r\n`；A/B/C 的 `.ini` 都存在且 0 字节；A 的战条**不入表**；B 的战条 `war_tick=now, war_time=0`；`MissingListFile` ⇒ 空表 + ERR、不 panic | `guild_exists` 用初始名单快照 ⇒ A 的战条出现（红）；按字母排序 ⇒ 序红；不建 `.ini` ⇒ 红；缺名单当致命 ⇒ 红 |
| T2 | `g3_file_bytes_roundtrip` | 载入 A 后 `save_guild`：逐字节 = §1.3（GBK、每行 CRLF、`公告/ /敌对行会/ /联盟行会/ /行会成员`、`+`/`#`、末行也 CRLF）；**敌对行会行被删**（`n14<=0`）；再 `load_guild` 回读 body 相等 | `\r\n`→`\n`；漏 `" "` 分隔行；保留战表行；末行不写 CRLF ⇒ 红 |
| T3 | `g3_run_10s_gate_strict` | 以 200ms 步进驱动：`run` 只在 `now-run_tick > 10000` 触发（首跳 10200，**不是** 10000；次跳 20400）；战争到期 `(now-war_tick) > war_time`：`war_time=50, war_tick=100` ⇒ 150 未完、151 完 | `>`→`>=`（10000 就跳/150 就完）⇒ 红；用 `now_ms()` 代替 `clock_ms` ⇒ 红 |
| T4 | `g3_run_30s_save_two_origins` | 加载后（`bo_changed=true, dw_save_tick=0`）：`origin=0` ⇒ 10200/20400 **无** Save、30600 首次 Save；`origin=3_600_000` ⇒ 10200 首次 Save；Save 后 `bo_changed=false`、下一跳不再 Save | `>=`；Save 前就把 `bo_changed=false`；用 epoch origin（i32 负）⇒ 红 |
| T5 | `g3_war_end_save_event` | 一会两战、一条到期：事件序 = `WarEnded(敌)` → `Save{UpdateGuildFile}`；保存体里该战条已消失、另一条保留；`CheckSaveGuildFile` **不**再命中 | 只等 30s（文件里战条仍在）⇒ 红；Save 排在 WarEnded 前 ⇒ 序红 |
| T6 | `g3_change_point_immediate_saves` | 逐点断言 §2.4：`add_guild`（建 `.ini`、`<名>.txt` **不创建**、名单末尾新增）；`update_notice`（落盘但 `bo_changed` 保持 false）；`add_member`/`del_member`/`update_rank=0`（落盘且 `bo_changed=true, dw_save_tick=now`）；自删解散（两次空体 save + 一次名单写、**无 `.bak`**） | 公告路径用 `update_guild_file`（`bo_changed` 翻转）⇒ 红；新会建出 `.txt` ⇒ 红；加会不写名单 ⇒ 红；写出 `.bak` ⇒ 红 |
| T7 | `g3_del_human_obj_compare_and_save_order` | 成员 [b,c]：断 `a` ⇒ `b` 被清成空名、`c` 不动；断 `b`/`c`/`d` ⇒ 不动；清空后条目仍在、`count()` 不变、`is_member("")==false`；`bo_changed=true` 且 `dw_save_tick` 旧 ⇒ 落盘发生在清空**之前**（文件里仍有 `+b`），随后再存才出现 `+\r\n` | `==1`→`!=0`（同名被清）⇒ 红；先清后存（文件缺 `+b`）⇒ 红 |
| T8 | `g3_disconnect_session_order` | 驱动 `GateEvent::Closed`：退组帧在行会处理之前；`DelHumanObj` 在 `world.disconnect` 之前且能看到会话里的会名；断线**零**行会帧（无 750/42/100）；再次 Closed 为 no-op | 先 `ground_sessions.remove` 再查会名 ⇒ 找不到（红）；断线发 750 ⇒ 红 |
| T9 | `g3_member_binding_not_online` | 加载成员（`actor_id=None`）即使在线也**收不到** 104；本会话 `add_member` 的成员（`actor_id=Some`）在线 ⇒ 收到一条，`Recog=其 ActorId`、`Param=MakeWord(F,B)`、`Series=1`、尾 = GBK(text)；绑定成员离线 ⇒ 不发 | 用在线表当绑定 ⇒ 加载成员收到帧（红） |
| T10 | `g3_guildmsg_104_wire` | `ShowPreFixMsg=0` ⇒ 尾 = 纯文本；`=1` ⇒ `〖行会〗+文本`（若实现）；空文本 ⇒ `sm_guild_message_104` 返回 `None`（整帧不发）；前缀/颜色取自 `[String]/[Setup]` 实读值 | `Recog` 用发送者 id；颜色写死 default；空文本仍发 ⇒ 红 |

可选 T11 `g3_nonzero_server_index`：`SaveGuildList` no-op；`save_guild` 目标不存在 ⇒ `MissingSaveTarget` 被吞、不 panic。

### 4.4 真机窗口（R，本卡唯一占窗口项）

- 前置：`MIR2_CLOCK_MS=3600000`（两侧同起点，口径 6）；影子 `GuildBase/`（`GuildList.txt` + 两会 `.txt`）。
- 断言：启动后 +10.2s 两会 `<名>.txt` 各被重写**一次**（战表行消失、公告/成员逐字节不变）；
  `GuildList.txt` 字节不变；`.ini` 仍 0 字节、无 `.bak`。
- 掉线：成员下线 ⇒ `<名>.txt` 在 `run` 前**不变**（Compare==1 未命中时）；构造「成员名 > 断线名」⇒
  下次保存出现一行 `+\r\n`（且只在 `UpdateGuildFile` 或 30s 触发时才落盘）。
- tier=R：`window_required: true`（文件字节 + 时序）；窗口未跑时 review 必须写 `request_changes` 或
  `approve + window_required`，不得用「语义等价」盖过。
- 750 出队序（G2b）若并入本卡，同一窗口一起抓。

### 4.5 白名单登记（`mir2-rs/tests/parity/whitelist.md`，编号避让其他车道）

1. **T-9x 节拍量化**：world tick 200ms vs oracle 50ms 扫描 ⇒ 10s 门首跳 +10200（oracle +10050）、
   30s 边界最多晚 150ms；战争到期最多晚 200ms。无 RNG、无帧内容差；真机窗口用同一注入时钟对拍。
2. **T-9x `MIR2_CLOCK_MS` 必须小值**：未注入 ⇒ `dw_save_tick=0` 的首次 30s 保存落在 30600 而非 10200（§2.5）；
   epoch 量级注入会因 i32 回绕使该门永不触发。窗口协议强制小值。
3. **T-9x 顶号 `DelHumanObj` 同 tick**：oracle `MakeGhost` → 下一次 `ProcessPlayObjectData`（≤250ms）的 `Disappear`
   才清行会；Rust `drain_kicks` 同 tick 调。仅文件写入相位不同；且 `drain_kicks` 目前不建模 party `DelMember`（既有缺口）。
4. **T-9x 保存编码**：Rust 恒 GBK，oracle 按 BOM 嗅探（UTF-8/UTF-16 文件会被原编码重写）；部署件无 BOM ⇒ 不可观测。
5. **T-9x 加载失败降级**：`.ini` 创建失败时 oracle 抛到 `InitializeWorld` 的 catch（初始化中断），
   Rust 记 ERR 并保留空会表；仅「GuildDir 缺失但名单非空」可达。
6. **T-9x 绑定是加载期快照**：绑定 `actor_id` 在重登后可能被内核复用；oracle 发往旧 socket，Rust 按
   `actor_id` 找当前会话 ⇒ 极端情况下收件人不同。部署单区小规模下不可观测；窗口登记。

---

## 5. 复核命令（只读，已在本 worktree 跑过）

```bash
git rev-parse HEAD                                   # b0e618c33
grep -n "pub fn run\|pub fn del_human_obj\|check_save_guild_file\|pub fn update_guild_file" mir2-rs/crates/world/src/guild.rs
grep -n "pub fn load_guild_info\|pub fn save_guild_list\|pub fn save_guild\b\|MissingSaveTarget" mir2-rs/crates/data/src/guild.rs
grep -n "GateEvent::Closed\|process_switch_data(\|injected_origin\|ground_sessions.remove" mir2-rs/bins/game-svr/src/main.rs
sed -n '70,78p'   src/GameSrv/Word/Threads/SystemProcessor.cs
sed -n '78,167p'  src/M2Server/Guild/GuildManager.cs
sed -n '525,550p;584,621p' src/M2Server/Guild/GuildInfo.cs
sed -n '431,449p' src/M2Server/Guild/GuildInfo.cs          # SendGuildMsg 绑定
grep -n "PlayObject = \|MemberList\[j\].PlayObject" src/M2Server/Guild/GuildInfo.cs
sed -n '2129,2150p' src/M2Server/Player/PlayObject.Message.cs
sed -n '293,343p' src/OpenMir2/Common/StringList.cs        # BOM 嗅探 + FileMode.Open
sed -n '25,35p'   src/Modules/SystemModule/Conf/StringConf.cs   # [Guild] 六键
```

## 6. 残留物

- 仅本文件 `E:/tmp/laneB-guild-g3.md`。无 worktree、无 `CARGO_TARGET_DIR`、无编译产物、无进程；
  未写仓库、未碰 `E:/MirServer`、未进入 `/e/tmp/wg2`、`/e/tmp/wslots`。


---

## 评审员更正（2026-10-20，**以本节为准；按现状不能开工**）

- **§3.2 把 `Compare(...) == 1` 误读为「严格大于」**；T7 用例本身写错，须按 `string.Compare(...)==1`（oracle 口径）重写。
- **时钟域不一致**：G2 实测用 `now_ms()` 写 `dw_save_tick`，G3 用 `world.clock_ms` —— 两者不同域，必须先统一（否则 30s 门不触发/乱触发）。
- **4 条判据的牙不合格**（见评审原文），需重写为「删接线必红」。
- **R 窗口前提不成立**：C# 侧时钟注入（`MIR2_CLOCK_MS`）需 owner 授权，未获授权前 G3 的时序判据只能构造级。
- 六条关键更正（`[Guild]` 六键须补、成员绑定只在 Load/SetGuildInfo/AddMember、`UpdateGuildFile` 立即落盘须为战争到期发 Save、
  `DelHumanObj` 先 CheckSave 再清成员、10s/30s 严格 `>` 且 `MIR2_CLOCK_MS` 取小值、加载 WarTime 恒 0 会删敌对行会行）成立。
