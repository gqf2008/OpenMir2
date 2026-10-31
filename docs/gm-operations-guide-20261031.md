# 开区 GM 操作指南（2026-10-31）

- 卡：`openmir2-gm-ops-guide`（tier=L，docs-only，worker=`openmir2-dev-1`）
- 基线：`walgit/master` `e39484e22`；写集：仅本文档
- 回答 owner 的「gm工具有吗 要改游戏参数咋弄」
- 口径：行为等价优先于修 bug。oracle 把 GM 权限锁死、把若干命令体注释成空操作，本仓照抄；本文只描述「现在怎么用 / 为什么用不了」，不夹带修复。

## 1. 一句话结论

**GM 工具有，但分三条路，只有中间一条现网可用：**

| 路径 | 权限来源 | 现网结果 | 判据 |
|---|---|---|---|
| 客户端 `@` 直呼 | `PlayObject.Permission` | **恒 0** ⇒ 185 条里只有 perm 0 的 5 条可达，另加内建 `@help`；其余一律红字 `权限不够!!!` | `gmcmd.rs` 模块头「现网可达性」；`adminlist.rs` 模块头事实 2 |
| 脚本 `GMEXECUTE` | 动作内**临时置 10**，`finally` 还原 | **唯一可用通道**；但本仓脚本桥是裁剪视图，只有一部分族的副作用真落地（§3） | `crates/script/src/engine/actions.rs` `ActionOfGmExecute` / `PermissionGuard` |
| D-* 偏差 | 接 `GetHumPermission` 或另设认证 | 需 owner 裁决；只写建议不动手（§5） | `adminlist.rs` 模块头「本片边界」 |

- oracle 原样：`AdminList.txt` 装载是活的、读取是死的（`GetHumPermission` 唯一调用点整段被注释），部署文件本身也产不出有效行（缺 IP 段被 `LoadAdminList` 丢弃），`StartPermission=0`、Release 构建 ⇒ `Permission` 恒 0。
- 本仓 `GmSession::permission` 默认 0 是上述事实的落点，不是新 bug。

## 2. 脚本 `GMEXECUTE` 用法

### 2.1 语法与执行链

```text
GMEXECUTE <命令名> [参数1] [参数2] [参数3] [参数4] [参数5]
```

- 动作参数最多 6 槽：第 1 槽是**命令名**，后 5 槽是参数；动作把它们拼成一行 `@命令 参数1 … 参数5` 交给 `ProcessUserLineMsg`（与客户端输入同一条分发链）。
- 参数 2 若**恰好**是 `Self`（ASCII 大小写不敏感）⇒ 替换成执行脚本的玩家名；其余位置、其余写法（含 `<$USERNAME>`）一律**不替换**。
- **`GMEXECUTE` 不展开 `<$...>` 变量**：`<$STR(M3)>`、`<$CURRENTMAP>`、`<$USERNAME>` 会作为字面量进入参数。这是 oracle `ActionOfGmExecute` 直接拼接 `sParam1..sParam6` 的原样行为（变量展开只发生在普通脚本动作参数，且命名变量同样原样，见 `whitelist.md` T-165）。
- 命令名按 `OrdinalIgnoreCase` 查表 ⇒ ASCII 大小写不敏感；分隔符只有**单个 ASCII 空格**：参数串**内部**的连续空格会保留为空参（不是「跳过」），首个空格后的多余前导空格会被 `Trim` 吞掉（首个参数不会因此变空槽）。
- 脚本文件是 GBK；本文引用时已转 UTF-8 展示。

### 2.2 真实脚本示例（原文摘录，不发明语法）

**示例 A：清理执行者自己的背包**（`E:/MirServer/M2GameSvr/Envir/Market_Def/功能NPC/桃源-N109.txt`，节 `[@06]`）

```text
GMEXECUTE 清理垃圾 self
```

- `清理垃圾` = `ClearBagItemCommand`；第 2 槽 `self` 被替换成玩家名 ⇒ 清该玩家背包并回 `SM_DELITEMS(709)`（空背包零帧）。
- 这是部署里少数**能真正落地**的 GMEXECUTE：`pending_clear_bag` 由 sim 循环的 `collect_gm_clear_bag_pending` 每 tick 收口。
- 反例：同一动作在 `Market_Def/LEGM2.CC假人NPC/假人上线NPC-jiaren.txt` 的 `[@main]` 写成 `GMEXECUTE 清理垃圾 <$USERNAME>`，变量不展开 ⇒ 目标名是字面量 `<$USERNAME>` ⇒ 只回「现在不在线」红字。

**示例 B：调整装备附加属性**（`E:/MirServer/M2GameSvr/Envir/Npc_Def/比奇国王-0122.txt`，节 `[@QF]`）

```text
Gmexecute 改属性 3 2 <$STR(M3)>
```

- `改属性` = `SmakeItemCommand`，参数依次是：装备位 `0..=12`、属性槽 `0..=15`（`2` = `Desc[2]`，攻击类）、数值。
- 该行第三参是 `<$STR(M3)>`，按上条规则不展开 ⇒ `StrToInt` 得 0；oracle 同样如此。要让数值生效，同位置写十进制字面量（如 `15`）即可，语法不变。
- 落地链：命令体只装 `pending_use_item`，脚本跑完后由 `apply_pending_gm_use_item_in` / 每 tick 的 `collect_gm_use_item_pending` 写会话 + 重算 + 发 `SM_UPDATEITEM(203)`。

**示例 C：oracle 本来就是空操作（别指望有效果）**

```text
Gmexecute showeffect 78                     # Market_Def/QFunction-0.txt 节 [@StdModeFunc14]
Gmexecute 取消禁言 self                      # Market_Def/QFunction-0.txt 节 [@PlayLevelUp]
Gmexecute 禁言 self 10000                    # QuestDiary/游戏登陆/登陆脚本.txt 节 [@登陆设置]
```

- `showeffect`：oracle `ShowEffectCommand` 整个方法体被注释 ⇒ 恒零帧（`gmcmd.rs` 的 `NOTE_SHOW_EFFECT_ALL_COMMENTED`）。
- `取消禁言`：判空写成 `A || !A` 恒真 ⇒ 永远只回一条红色帮助；真正的解禁代码在 `EnterCriticalSection` 之后且被注释。
- `禁言`：`EnterCriticalSection(SystemShare.DenySayMsgList)` 触 null（该字段声明即 null、全仓零赋值）⇒ NRE 被 `PlayObject.Chat.cs` 顶层 catch 吞 ⇒ 零帧。

### 2.3 权限语义（临时置 10）

- 命令门在 `GameCommand.Handle`：`PlayerActor.Permission < Command.PermissionMin` ⇒ 返回 `权限不够!!!`；`GMEXECUTE` 只是把 `Permission` 临时改成 10，因而 **`0 < PermissionMin ≤ 10` 的命令经脚本可达**。
- 抬权只有一处：`actions.rs` 的 `PermissionGuard`（`Drop` 还原旧值，含异常路径）；宿主 `NpcPlayer::set_permission`/`process_user_line_msg` 不再二次抬权。
- 抬权只覆盖**这一行命令**，不改变会话的持久权限；脚本执行完 `Permission` 回到进入前的值。

## 3. 九族可用性分层（本仓实况）

**桥补全后**（卡 `openmir2-gmexe-bridge-channels`，合并于 master `b8e625682`）脚本桥不再只注入 `maps/world`：`ScriptGmHost`（`mir2-rs/bins/game-svr/src/lib.rs`）承载六组宿主通道（`quests`/`m_host`/`filter_word`/`mission`/`spirit_mutiny_tick`/`caller_guild`）与 `pose_target`，5 个生产装配点（`CM_EAT` 的 `@StdModeFunc*` / `CM_CLICKNPC` / 固定 NPC / 商店 `CM_*` / 组队 `CM_*`）全部用 `ScriptGmHost::for_event` 装配（经 `ScriptGmEnv::bridge` 传入命令体）。**范围限定**：这 5 个入口之外的**第 6 个宿主入口 `@Login`** 仍自建 `GmWorld`（`quests=None`/`m_host=false`/`caller_guild=None`/`pose_target=None`，仅注入 `filter_word`/`mission`/`spirit_mutiny_tick`）⇒ 依赖这四项的命令在 `@Login` 脚本内仍退化零帧；每个命令事件末尾由 `command_event_tail_gm_drain` → `drain_script_gm_pendings` 按与 `CM_SAY` 支同序收口（①`apply_pending_gm_entity`/②`apply_pending_level_up`/③改属性/④清理垃圾/⑤`apply_pending_gm_feature_changed`/⑥`reconnect`/⑦`apply_pending_gm_target_ops`/⑧`apply_pending_gm_m_commands`/⑨`apply_pending_gm_castle_commands`/⑩`apply_pending_gm_guild_commands`）。下表「经 GMEXECUTE（装配后）」按这 5 个 `for_event` 入口上的真实副作用写。

**退化兜底保留**：未装配 `ScriptGmHost`（`ScriptGmHost::default`，脚本桥未接线或部分测试夹具）时六组通道为 `None`/`false`，命令体仍走 `log_note` 零帧、不伪造写；该形态由 `script_bridge_unwired_keeps_degenerate_fallbacks` 钉住，不是现网。

| 族 | 条数 | 命令面与 oracle 事实 | 经 GMEXECUTE（装配后） | 证据（路径 + 符号） |
|---|---:|---|---|---|
| P 密码锁门 | 7 | 部署 `PasswordLockSystem=0` ⇒ 7 条第一句恒红「游戏密码保护系统还没有启用!!!」，零状态 | 抬权后同样只回红字；启用分支现网不可达（oracle 的部署门，桥通道救不了） | `gmcmd.rs` `Imp::PasswordLockGate`；报告 §2（九族总表 P）/ §4（P 共享门死支） |
| T 会话开关/骑马 | 7 | 4 条会话开关（`@交易`/`@加入门派`/`@拒绝组队聊天`/`@禁止行会信息`）翻会话态并回绿字；`@OnHorse` 因 `HorseType` 恒 0 只回「骑马必须先戴上马牌!!!」；`@OffHorse` 未骑马静默；`@DisableFilter` 翻全局 `FilterWord` | **真副作用**：4 条开关回绿字且会话镜像翻转（`ban_shout` 有真实消费点）；`@DisableFilter` 翻转全局 `FilterWord`（`filter_word` 通道注入）；`@OnHorse`/`@OffHorse` 成功分支的 `SM_FEATURECHANGED(41)` 外观帧经 drain 步骤⑤广播（部署下 `HorseType` 恒 0 ⇒ `@OnHorse` 仍只回红字、`@OffHorse` 未骑马静默，oracle 原样） | `gmcmd.rs` `take_on_horse`/`disable_filter`/`Imp::LetTrade`；`lib.rs` `ScriptGmHost.filter_word`/`apply_pending_gm_feature_changed`；`script_bridge_drain_feature_changed_broadcasts`；报告 §2（T 行）/ §4（T 骑马） |
| Q 查询/展示 | 10 | 只读；含 oracle 怪癖：`@IP` 判空恒真恒红帮助、`@信息` 在线零帧、`@Showuseiteminfo` 单字符名越界零帧且 `%s` 字面输出、`@Map` 缺参越界零帧 | maps/world/quests 均已注入 ⇒ 查询可用；`@showflag` 读真实任务位快照（不再恒走 OFF 支；未装配时仍按全零模型走 OFF 绿字支） | `gmcmd.rs` `show_flag`/`human_local`/`show_use_item_info`；`lib.rs` `GmWorld::quest_of`；报告 §2（Q 行）/ §4（Q 展示怪癖） |
| X1 禁言名单 | 5 | `DisableSendMsgList`/`DenySayMsgList` 声明即 null、全仓零赋值 ⇒ 触表四命令真机 NRE 被吞 = 零帧 | 与桥无关：`@+jy`（非空）/`@-jy`/`@+-jy`/`@查看禁言` 仍零帧；`@查看私聊` 正常双绿字（只触已初始化数据） | `gmcmd.rs` `disable_send_msg`/`enable_send_msg`/`view_whisper`；报告 §3.2（A2 偏差清单）/ §4（X1 四命令体零帧） |
| X2 目标控制/杂项 | 18 | 10 条文本/查询/全局开关（`@DelNpc`/`@清除任务`/`@清除怪物`/`@Mission`/`@祈祷生效`/`@停止叛变`/`@探测`/`@fdht7ed`/`@sdasdert`/`@Reconnection`）；6 条重副作用写面；2 条宿主脚本接缝（`@是我` oracle 有帧 / `@MemberFuncEx` 部署无 label 零帧） | **真副作用**：`@Mission`/`@祈祷生效`/`@停止叛变` 写全局单元（`mission`/`spirit_mutiny_tick` 通道）；`@vv杀死`（无参面对面支靠 `pose_target`）/`@复活`/`@VV给予权限`/`@删除技能`/`@ClearCopyItem`/`@清除仓库密码` 经 drain 步骤⑦落地；`@Reconnection` 的 `SM_RECONNECT` 经步骤⑥入重连队列；其余文本/查询可用。**两条 `no_frames` 拆开归因**：`@是我`（`ManageNPC.GotoLable("@Member")`）是**本仓缺口**——宿主脚本接缝未接，oracle 可达且有帧（部署 `Envir/MapQuest_def/QManage.txt:40 [@Member]`：`ISADMIN` → `mapmove N109 40 40` + `SENDMSG`）；`@MemberFuncEx`（`FunctionNPC.GotoLable("@Member")`）则**oracle 本部署亦零帧**——`FunctionNPC` 的脚本 `Envir/Market_Def/QFunction-0.txt` 57 个 label **无 `[@Member]`**（命令体非空，是部署脚本无对应 label，非「本体空」也非纯本仓缺口） | `gmcmd.rs` `kill_cmd`/`realive`/`set_permission`/`mission_cmd`/`spirt_start`/`spirt_stop`；`lib.rs` `ScriptGmHost.pose_target`/`drain_script_gm_pendings` 步骤⑥⑦；`script_bridge_pose_target_feeds_vvkill_face_branch`/`script_bridge_drain_reconnect_enqueues`；报告 §2（X2 行）/ §4（X2 复活/删除技能/ClearCopyItem）；oracle 路由 `src/Modules/GameCommand/Commands/MemberFunction{,Ex}Command.cs`（`ManageNPC`/`FunctionNPC`）、`src/GameSrv/DB/LocalDB.cs` `QMangeNpc`/`QFunctionNpc`；部署 `QManage.txt:40 [@Member]` 与 `QFunction-0.txt` 57 label 无 `[@Member]`（GBK iconv grep） |
| F 脚本标志 | 3 | `@setflag`/`@setopen`/`@setunit` 写**调用者**任务位（不是目标）并对目标回绿字；任务位**不落库**（oracle `Quest*` 运行时态，`bins/db-srv/tests/quest_no_persist.rs`） | **真副作用**：`quests` 快照注入后可做 oracle 的写后读回，经 drain 步骤⑦落调用者 `saves.record.data.quest_*`（运行时真身，仍**不落库**）并给目标回绿字 | `gmcmd.rs` `set_quest_bits`；`lib.rs` `GmWorld.quests`/`apply_pending_gm_target_ops`（任务位段）；`script_bridge_quests_write_caller_flags_through_host_drain`；报告 §2（F 行）/ §4（F 写调用者怪癖） |
| M 刷怪/造物 | 6 | `@MobNpc`/`@MobPlace`/`@ReCallMobEx`/`@怪`/`@GiveMine`/`@s`；含 RNG 抽数（`Random(18)+3`、每件 `Random(MakeRandomAddValue)`） | **真副作用（5/6）**：`@MobPlace`/`@ReCallMobEx`/`@怪`/`@GiveMine`/`@s` 的 `m_host` 门放开，经 drain 步骤⑧走宿主 `MonGenHost` + `&mut World` + 与世界同一条 RNG 流（刷怪/造物真落地）。`@MobNpc` 无 `m_host` 门且 oracle 建 NPC 段整段注释 ⇒ 缺参/`?` 红帮助、足参静默零帧（照抄，不补） | `gmcmd.rs` `mob_npc`/`mob_place`/`recall_mob_ex`/`mob_spawn`/`give_mine`/`make_item`；`lib.rs` `GmWorld.m_host`/`apply_pending_gm_m_commands`；`script_bridge_m_host_mobplace_spawns_real_monster`/`script_bridge_drain_smake_lands_use_item_write`；报告 §2（M 行）/ §4（M I1/RNG/T-271） |
| C 城堡/赛事 | 5 | `@ContestPoint` 只读；`@开始攻城`/`@SabukWallGold`/`@沙城`/`@StartContest` 写城堡/赛事；三处源码文本不可达（无参 SbkGold / `GetCastle` NotImpl / 零 cry） | **真副作用（3/5）**：`@开始攻城`/`@SabukWallGold`/`@StartContest` 的 `m_host` 门放开，经 drain 步骤⑨落 TotalGold/UnderWar/门格掩码/FightZoneDieCount。`@ContestPoint` 文字可用；`@沙城` 无 `m_host` 门但 oracle 本身 `NotImplementedException` 零帧（照抄） | `gmcmd.rs` `sbk_gold`/`forced_wallconquest_war`/`start_contest`/`change_sabuk_lord`；`lib.rs` `apply_pending_gm_castle_commands`；`script_bridge_drain_castle_gold_lands`；报告 §2（C 行）/ §4（C 三处源码不可达/C I1/C 怪癖） |
| W 行会 | 4 | `@创建行会`/`@删除行会`/`@退出门派`/`@允许天地合一`；`AllowGroupReCall` 是独立字段；落库有 T-272 oracle 原样 bind | **真副作用（4/4）**：`@创建行会`/`@删除行会`/`@退出门派` 的 `m_host` 门放开（`@退出门派` 另需注入的 `caller_guild` 快照与 `GuildRankNo` 门），经 drain 步骤⑩落 GuildMgr 写 + 文件/列表落盘 + 750/42/100 帧序。`@允许天地合一` **无** `m_host` 门，新值装 `pending_entity`，经 drain 步骤①落权威 `Entity.allow_group_re_call` 并由存档派生写 `characters.AllowGroupReCall` | `gmcmd.rs` `add_guild`/`del_guild`/`end_guild`/`allow_group_recall`；`lib.rs` `GmWorld.caller_guild`/`apply_pending_gm_entity`/`apply_pending_gm_guild_commands`；`script_bridge_caller_guild_ends_guild_through_host_drain`/`script_bridge_allow_group_recall_writes_entity_and_record`；报告 §2（W 行）/ §4（W I1/I2 / T-272） |

> 另有 9 条「已对齐」（`ClearItemMap`/`EndContest`/`ServerInfo`/`SetMapMode`/`showopen`/`showunit`/`Total`/`who`/`查看周围`）：oracle 过权限门后恒零帧，本仓按 `no_frames` + `log_note` 照抄，不计入 65 条。
>
> 结论：**桥补全后，脚本 GMEXECUTE 已接通与客户端直呼同一套宿主通道与 `pending_*` 收口**。在装配 `ScriptGmHost::for_event` 的 5 个入口上（`@Login` 除外，见本节范围限定），M 刷怪/造物 5 条、C 城堡/赛事 3 条、W 全 4 条、F 任务位 3 条、X2 的目标控制重副作用与 `@Mission`/`@祈祷生效`/`@停止叛变`、T 的 `@DisableFilter` 与外观帧都**真落地**。仍零帧/只回文字的项分两类：① oracle 本身零帧、空体或不可达（`@MobNpc`、`@沙城`、P 全族、X1 触表命令、X2 的部分怪癖，以及 **`@MemberFuncEx`**——`FunctionNPC` 的 `QFunction-0.txt` 57 个 label 无 `[@Member]`，命令体非空但部署脚本无对应 label ⇒ oracle 本部署零帧）；② **本仓缺口** `@是我`——`ManageNPC.GotoLable("@Member")` 宿主脚本接缝未接，oracle 可达有帧（部署 `QManage.txt:40 [@Member]` 可证），不计入 oracle 归零项。**未装配 `ScriptGmHost` 时仍退化 `log_note` 零帧、不伪造写**（兜底形态，不是现网）。

## 4. 开区改游戏参数

### 4.1 生效方式

- `E:/MirServer/M2GameSvr/Server.conf` 的 `[Setup]` 在 GameSrv 启动时由 `ServerConf` 读入 ⇒ **改完必须重启 GameSrv**。
- 没有通用「热重载 Server.conf」命令。`@重读爆率`（`ReloadMonItemsCommand`）的 try 体整段被注释，只回「怪物爆物品列表重加载完成...」，**不会真的重读**。

### 4.2 高频参数（部署值 / oracle 缺省 / 含义 / 生效点）

| 键（`[Setup]`） | 部署值 | 缺省 | 含义与生效点 |
|---|---:|---:|---|
| `MonsterPowerRate` | 10 | 10 | 怪物 HP/MP/AC/MAC/DC/DCMAX/MC/SC 按 `rate/10` 缩放（`MySqlDB.LoadMonsterDB`）；**例外**：`Race` 为 `SabukWall`/`SabukDoor` 时 HP 不乘（仅 HP；MP/AC/MAC/DC/DCMAX/MC/SC 仍乘）。**怪物模板装载时**生效，重启才变 |
| `GenMonRate` | 10 | 10 | 刷怪目标数除数：`目标数 = max(1, round(max(1, Count) / (rate/10)))`（`WorldServer.MonGen.cs` 的刷怪循环 `genModCount`；Rust `crates/world/src/mongen.rs`）。**值越大刷得越少**；重启生效 |
| `MonGenRate` | 1 | — | **死键**：oracle/Rust 读的是 `GenMonRate`；`MonGenRate=1` 不参与计算（部署文件同时存在两键，以 `GenMonRate=10` 为准） |
| `MonUpLvRate` | 16 | 16 | 宠物/宝宝升级所需杀怪数（`MonsterObject.cs`）；重启生效 |
| `MonRandomAddValue` | 40 | 10 | 掉落「极品」判定：`Random(MonRandomAddValue) == 0` ⇒ 1/40；重启生效 |
| `MakeRandomAddValue` | 100 | 10 | `@s` 造物每件一次 `Random(MakeRandomAddValue)`（品质抽数）；重启生效 |
| `DropItemRage` | 未写 | 3 | **爆物散布半径**，不是爆率；按调用点取 `MIN(值,3)` 或 `MIN(值,7)` |
| `ItemPowerRate` | 10000 | 10000 | **解析但无消费点**（oracle 全仓只声明+读取，无使用）⇒ 改了不生效 |
| `ItemExpRate` | 10000 | 10000 | 同上，**无消费点** |
| `PKDieLostExpRate` | 1000 | 1000 | 同上，**无消费点** |

> 部署里还有 `SaveKillMonExpRate`/`SaveHumExpRate`/`RecommendExpRate`/`LimitExpRate*` 等键：两端源码都不读，属部署遗留，改了不生效。

### 4.3 爆率怎么改

- 爆率**不在 Server.conf**，按怪物写在 `E:/MirServer/M2GameSvr/Envir/MonItems/<怪名>.txt`（GBK），行格式：

```text
<分子>/<分母>    <物品名>    [数量]
10/100          金币        300
10/3000         海魂
```

- 判定是 `Random(分母) <= 分子-1`（`WorldServer.MonGen.cs` `MonGetRandomItems`；装载在 `LocalDB.LoadMonitems`，Rust 在 `mir2-rs/crates/data/src/monitems.rs`）⇒ 上例「10/100」≈ 10%，「10/3000」≈ 0.33%。`金币` 是特殊名，按数量的一半 + `Random(数量)` 掉金币。
- 改完**重启**才生效；运行中新刷的怪也不会读新文件（`@重读爆率` 是空操作）。

### 4.4 经验倍率怎么改

- **没有全服经验倍率键**。`KillMonExpRate` 不是 `Server.conf` 键，而是每个玩家身上的字段（`PlayObject.KillMonExpRate`，ctor = 100）。
- 在线、按玩家：脚本动作 `KILLMONEXPRATE <倍率> <秒数>`（`ExecutionProcessingSys.ActionOfKillMonExpRate`；`100` = 1 倍，`200` = 2 倍，`0` 也合法），到期由 `PlayObject.Message` 的秒检查还原 100。Rust 已接 `ActionOfKillMonExpRate`。
- 按地图：`Envir/MapInfo.txt` 的地图标志 `EXPRATE(n)`（`Maps.cs`；`n` 是百分比，`EXPRATE(200)` = 2 倍），启动装载，重启生效。
- 怪物自身经验来自 `monsters` 表 + 地图/玩家倍率，改 DB 需重启。

## 5. 若 owner 需要客户端 `@` 直呼（D-* 偏差路径，只建议不动手）

> **桥补全后同步**：脚本 `GMEXECUTE` 已能落地与客户端直呼同源的真副作用（§3；`@Login` 入口与 `@MemberFuncEx`/`@是我` 除外——后者是脚本接缝缺口、前者 oracle 本部署无 `[@Member]` label），开区 GM 操作**不再依赖**客户端直呼；D-* 只是 owner 想要「GM 直接敲 `@` 命令」这一交互形态时的选项，优先级可降，但仍为 owner 选项。

现状：客户端直呼的 `CM_SAY` 路径其实**已经具备完整视图**——`handle_say_with_saves` 注入 `quests: Some(...)`、`m_host: true`、`filter_word/mission/spirit_mutiny_tick/caller_guild`，并在命令后同一事件内调用 `apply_pending_gm_target_ops`/`_m_commands`/`_castle_commands`/`_guild_commands`。**唯一缺的是权限来源**。两条方案：

1. **接 `GetHumPermission`（推荐先评估）**：在登录支调用 `AdminList::get_hum_permission`，把命中级写到 `GmSession.permission`；同时修 `Envir/AdminList.txt`（必须有「名字 + IP 段」的有效行，当前单行缺 IP 会被 `LoadAdminList` 丢弃）。这是**偏差**（oracle 的调用点被注释），须开 `D-*` 卡并登记 `whitelist.md`。收益：一次接通，九族命令体在客户端直呼路径全部可达（含 oracle 原样的恒红/零帧项，不再是权限门挡下）。
2. **另设 GM 认证**：例如控制台/一次性令牌/独立管理端口，把认证结果写 `GmSession.permission`。改动面更大、可观测面更多，且与 oracle 无对应物，建议仅在方案 1 不可行时考虑。

> 只修 `AdminList.txt` 不接 `GetHumPermission` 仍然无效（读取调用是死的）；反过来只接调用不修文件也永远查不到行。两半必须一起做。

## 6. 证据索引（路径 + 符号，无行号锚）

- 实现收口：`docs/gm-implementation-report-20261031.md` §1/§2/§3/§4
- 命令表与权限门：`mir2-rs/bins/game-svr/src/gmcmd.rs`（`COMMANDS`/`CommandDef`/`GmSession`/`GmWorld`/`execute`/`PERMISSION_TOO_LOW`）
- 脚本抬权：`mir2-rs/crates/script/src/engine/actions.rs`（`ActionOfGmExecute`/`PermissionGuard`）
- 宿主跳线：`mir2-rs/bins/game-svr/src/npc.rs`（`NpcGm`/`NpcPlayer::process_user_line_msg`/`set_permission`）
- 脚本桥裁剪视图与 pending 收口：`mir2-rs/bins/game-svr/src/lib.rs`（`ScriptGmEnv::bridge`、`handle_say_with_saves`、`apply_pending_gm_*`、`collect_gm_kick_pending`/`collect_gm_use_item_pending`/`collect_gm_clear_bag_pending`）
- 权限锁死事实链：`mir2-rs/bins/game-svr/src/adminlist.rs` 模块头、`AdminList::get_hum_permission`
- 脚本可达清单：`mir2-rs/tests/parity/coverage/gm_registry.json`（`script_occurrences`/`unknown_script_commands`/`review_corrections`）
- 部署参数：`E:/MirServer/M2GameSvr/Server.conf`；`E:/MirServer/M2GameSvr/Command.conf`
- 爆率文件与装载：`E:/MirServer/M2GameSvr/Envir/MonItems/*.txt`；`src/GameSrv/DB/LocalDB.cs` `LoadMonitems`；`src/GameSrv/Word/WorldServer.MonGen.cs` `MonGetRandomItems`；`mir2-rs/crates/data/src/monitems.rs`
- 经验倍率：`src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs` `ActionOfKillMonExpRate`；`src/GameSrv/Maps/Maps.cs` `EXPRATE`；`src/M2Server/Player/PlayObject.Base.cs` `KillMonExpRate`
