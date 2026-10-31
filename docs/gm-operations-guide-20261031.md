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
- 命令名按 `OrdinalIgnoreCase` 查表 ⇒ ASCII 大小写不敏感；分隔符只有**单个 ASCII 空格**（连续空格会产生空参，不是「跳过」）。
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

「经 GMEXECUTE」一列按本仓脚本桥 `ScriptGmEnv::bridge` 的真实能力写：它注入 `maps/world`，但 `quests=None`、`m_host=false`、`filter_word/mission/spirit_mutiny_tick/caller_guild` 均未注入。命令体若依赖这些单元，会走 `log_note` 零帧或有回执无副作用。

| 族 | 条数 | 命令面与 oracle 事实 | 经 GMEXECUTE 的现状（本仓） | 证据 |
|---|---:|---|---|---|
| P 密码锁门 | 7 | 部署 `PasswordLockSystem=0` ⇒ 7 条第一句恒红「游戏密码保护系统还没有启用!!!」，零状态 | 抬权后同样只回红字；启用分支现网不可达 | `gmcmd.rs` `Imp::PasswordLockGate`；报告 §2-P |
| T 会话开关/骑马 | 7 | 4 条会话开关（`@交易`/`@加入门派`/`@拒绝组队聊天`/`@禁止行会信息`）翻会话态并回绿字；`@OnHorse` 因 `HorseType` 恒 0 只回「骑马必须先戴上马牌!!!」；`@OffHorse` 未骑马静默；`@DisableFilter` 翻全局 `FilterWord` | 4 条开关回绿字且会话镜像翻转（`ban_shout` 有真实消费点）；`@OnHorse` 红字可达；`@DisableFilter` 因全局位未注入 ⇒ 零帧；`SM_FEATURECHANGED(41)` 外观帧只在客户端直呼路径排水 | `gmcmd.rs` `take_on_horse`/`disable_filter`/`Imp::LetTrade`；报告 §2-T |
| Q 查询/展示 | 10 | 只读；含 oracle 怪癖：`@IP` 判空恒真恒红帮助、`@信息` 在线零帧、`@Showuseiteminfo` 单字符名越界零帧且 `%s` 字面输出、`@Map` 缺参越界零帧 | 地图/世界视图已注入 ⇒ 绝大多数查询可用；`@showflag` 因无任务位快照 ⇒ 恒走 OFF 绿字支 | `gmcmd.rs` `show_flag`/`human_local`/`show_use_item_info`；报告 §2-Q |
| X1 禁言名单 | 5 | `DisableSendMsgList`/`DenySayMsgList` 声明即 null、全仓零赋值 ⇒ 触表四命令真机 NRE 被吞 = 零帧 | `@+jy`（非空）/`@-jy`/`@+-jy`/`@查看禁言` 均零帧；`@查看私聊` 正常双绿字（只触已初始化数据） | `gmcmd.rs` `disable_send_msg`/`enable_send_msg`/`view_whisper`；报告 §3.2 |
| X2 目标控制/杂项 | 18 | 10 条文本/查询/全局开关（`@DelNpc`/`@清除任务`/`@清除怪物`/`@Mission`/`@祈祷生效`/`@停止叛变`/`@探测`/`@fdht7ed`/`@sdasdert`/`@Reconnection`）；6 条重副作用写面；2 条宿主脚本接缝 | 纯文本/查询多数可用；`@Mission`/`@祈祷生效`/`@停止叛变` 因全局单元未注入 ⇒ 零帧；`@vv杀死`/`@复活`/`@VV给予权限`/`@删除技能`/`@ClearCopyItem`/`@清除仓库密码` 只回文字（`@复活`/`@VV给予权限`/`@清除仓库密码`）或零帧，**副作用因 `pending_*` 不在脚本桥排水而不落地**；`@MemberFuncEx`/`@是我` 零帧；`@Reconnection` 的 `SM_RECONNECT` 只在客户端直呼路径排水 | `gmcmd.rs` `kill_cmd`/`realive`/`set_permission`；`lib.rs` `apply_pending_gm_target_ops` 仅在 CM_SAY 分支调用；报告 §2-X2 |
| F 脚本标志 | 3 | `@setflag`/`@setopen`/`@setunit` 写**调用者**任务位（不是目标）并对目标回绿字；quest 数组不落库 | 脚本桥 `quests=None` ⇒ 三条全部 `log_note` 零帧（部署脚本无这三条的可达调用） | `gmcmd.rs` `set_flag`/`set_open`/`set_unit`；报告 §4-F |
| M 刷怪/造物 | 6 | `@MobNpc`/`@MobPlace`/`@ReCallMobEx`/`@怪`/`@GiveMine`/`@s`；含 RNG 抽数（`Random(18)+3`、每件 `Random(MakeRandomAddValue)`） | 六条都以 `if !world.m_host` 开头 ⇒ 脚本桥 `m_host=false` ⇒ 全部 `log_note` 零帧。刷怪/造物需宿主 `MonGenHost` + `&mut World` + RNG 通道，脚本桥没有 | `gmcmd.rs` `mob_spawn`/`make_item`/`give_mine` 的 `m_host` 门；报告 §4-M |
| C 城堡/赛事 | 5 | `@ContestPoint` 只读；`@开始攻城`/`@SabukWallGold`/`@沙城`/`@StartContest` 写城堡/赛事；三处源码文本不可达（无参 SbkGold / `GetCastle` NotImpl / 零 cry） | `@ContestPoint` 文字可用；`@开始攻城`/`@SabukWallGold`/`@StartContest` 走 `m_host` 门 ⇒ 脚本桥零帧；`@沙城` 无 `m_host` 门但本身是 oracle `NotImplementedException` 零帧；宿主落地（TotalGold/UnderWar/门格掩码/FightZoneDieCount）只在客户端直呼路径 | `gmcmd.rs` `sbk_gold`/`change_sabuk_lord`/`forced_wall_war`；`lib.rs` `apply_pending_gm_castle_commands` 仅在 CM_SAY 分支；报告 §4-C |
| W 行会 | 4 | `@创建行会`/`@删除行会`/`@退出门派`/`@允许天地合一`；`AllowGroupReCall` 是独立字段；落库有 T-272 oracle 原样 bind | 四条命令体同样 `m_host` 门 ⇒ 脚本桥零帧；宿主落地（GuildMgr 写 + 文件/列表落盘 + 750/42/100 帧序）只在客户端直呼路径 | `gmcmd.rs` `add_guild`/`del_guild`/`end_guild`；`lib.rs` `apply_pending_gm_guild_commands` 仅在 CM_SAY 分支；报告 §4-W |

> 另有 9 条「已对齐」（`ClearItemMap`/`EndContest`/`ServerInfo`/`SetMapMode`/`showopen`/`showunit`/`Total`/`who`/`查看周围`）：oracle 过权限门后恒零帧，本仓按 `no_frames` + `log_note` 照抄，不计入 65 条。
>
> 结论：**脚本 GMEXECUTE 今天真正能落地的只有「改属性 / 清理垃圾 / VV踢人 / 查询类文字 / 会话开关回执」这一小片**；M/C/W/F 与 X2 重副作用是「命令体与宿主落地已实现，但脚本桥没接写通道」。要让它们经脚本可用，需要把对应的 `pending_*` 收口接进脚本执行路径；这不是内容问题，是宿主接线问题。

## 4. 开区改游戏参数

### 4.1 生效方式

- `E:/MirServer/M2GameSvr/Server.conf` 的 `[Setup]` 在 GameSrv 启动时由 `ServerConf` 读入 ⇒ **改完必须重启 GameSrv**。
- 没有通用「热重载 Server.conf」命令。`@重读爆率`（`ReloadMonItemsCommand`）的 try 体整段被注释，只回「怪物爆物品列表重加载完成...」，**不会真的重读**。

### 4.2 高频参数（部署值 / oracle 缺省 / 含义 / 生效点）

| 键（`[Setup]`） | 部署值 | 缺省 | 含义与生效点 |
|---|---:|---:|---|
| `MonsterPowerRate` | 10 | 10 | 怪物 HP/MP/AC/MAC/DC/DCMAX/MC/SC 按 `rate/10` 缩放（`MySqlDB.LoadMonsterDB`）；**怪物模板装载时**生效，重启才变 |
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
