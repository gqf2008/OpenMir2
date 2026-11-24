> 原路径 E:/UserTemp/a34-slice4-writeset.md（2026-11-24 入仓，内容未改）
> 入仓时锚已规范化（oracle `src/**` 行号保留、本仓高 churn 文件改符号锚），结论与事实未改；原文锚对应 `250c8a97d`。
# A34 片 4（行会 9 条，Process 族）——写集与依赖地图

> 只读准备件（不写码、不编译）。基准：master `2798afde1`（A20 已合；A34 片 3 由 `a35-b-e6` 在做，本文按其
> `E:/UserTemp/a34-slice3-writeset.md` 口径标冲突）。本文件只在 `E:/UserTemp`，不入库。
> oracle = 主检出 `src/**`（C#）；Rust = `mir2-rs/`（当前树与 master 一致于相关文件）。

---

## 0. 现状（区 k 行为）

- `zone_cmd_class`：9 条 `CM_GUILD*`（`guild_cmd::GUILD_IDENTS`，显式 9 条、**不是** `1035..=1045`：
  1042 非法日志、1043 是 `CM_ADJUST_BONUS`）**不在** `ident_level` 链，也不在 `CM_SAY`/脚本两族
  ⇒ 落表外 `Unsupported`。
- `handle_zone_command_on` 尾部：`note_zone_cmd_unsupported(cmd.ident)` + `ZoneCmdOutcome::Unsupported`
  ⇒ **丢弃 + 计数 `zone_cmd_unsupported`，零帧**。
- 区 0 侧：`INBOUND_PROCESS` 里 `guild_cmd::GUILD_IDENTS.contains(&cmd.ident)` 臂（`lib.rs`
  `handle_guild_cmd(...)` 调用点）已实现 9 条；**只读**该分支即可得到「区 0 可执行 = 全部输入都在」的
  参数清单。
- 现有测试：`lib.rs` 内**没有**把 guild ident 当 `Unsupported` 的守恒断言（已 grep 确认），片 3 也不动它们。

---

## 1. 逐命令表（9 行）

字段口径：`CM_*` 12B 头（recog/param/tag/series）+ 尾 `Msg`（名字/文本，`.Trim()`）；全部 **无 DB 请求**，
持久化 = `guild_runtime::save_one/save_record/save_list`（进程级 `GuildStore` 文件，`Option<&GuildStore>`，
`None` ⇒ 静默不落盘，与区 0 现状同）。

| # | ident / 名字 | oracle 处理器（`src/**`） | 现区 k | 依赖的**进程态** | 建议归属 | 依赖卡 |
|---|---|---|---|---|---|---|
| G1 | `CM_OPENGUILDDLG(1035)` | `PlayObject.Operate.cs:1229-1277` `ClientOpenGuildDlg` | Unsupported（丢+计数） | `GuildManager`（MyGuild/名册/排名）、`GuildMsgCfg`、`GuildConfig.ref_msg_range`、会话 `guild` 缓存 | **Process**（A6 控制邮箱） | A6（已有） |
| G2 | `CM_GUILDHOME(1036)` | 同上 `:1278-1281`（= 1035） | 同上 | 同上 | **Process** | A6 |
| G3 | `CM_GUILDMEMBERLIST(1037)` | `:1283-1305` `ClientGuildMemberList` | 同上 | `GuildManager` 名册（**按名册拼文本**；在线与否只影响…见 §3） | **Process** | A6 |
| G4 | `CM_GUILDADDMEMBER(1038)` | `:1307-1367` `ClientGuildAddMember` | 同上 | `GuildManager`（add_member/排名上限）、目标**会话缓存**写（`SessionGround.guild`）、`GuildStore` 落盘、`online`（目标 actor）、`displays`/`world`（42）、绿字配置 | **Process** | A6；**跨区名册缺口**见 §5 |
| G5 | `CM_GUILDDELMEMBER(1039)` | `:1369-1424` `ClientGuildDelMember` | 同上 | `GuildManager`（del_member/cancel_guld/del_guild）、目标/自己会话清空、落盘/落列表、`online`/`displays`/`world` | **Process** | A6；跨区缺口 |
| G6 | `CM_GUILDUPDATENOTICE(1040)` | `:1426-1442` `ClientGuildUpdateNotice` | 同上 | `GuildManager`（notices）、落盘；随后 `open_dlg`（G1） | **Process** | A6 |
| G7 | `CM_GUILDUPDATERANKINFO(1041)` | `:1444-1463` `ClientGuildUpdateRankInfo` | 同上 | `GuildManager`（update_rank，需 online 判在线）、落盘 | **Process** | A6 |
| G8 | `CM_GUILDALLY(1044)` | `:1465-1523` `ClientGuildAlly` | 同上 | `GuildManager`、`pose_create`（前排）、`online`、对方会话 `guild_name`；**生产恒 `769/-4`**（`EnableAuthAlly` 恒 false） | **Process** | A6 |
| G9 | `CM_GUILDBREAKALLY(1045)` | `:1525-1560` `ClientGuildBreakAlly` | 同上 | `GuildManager`（掌门门）；**生产恒 `769/0`**（`IsAllyGuild` 恒 false；`771` 无发点） | **Process** | A6 |

**为什么是 Process 而不是 Local / 新通道**：9 条全部读写 `GuildManager`（进程级、A9 口径不可复制到区 k）
或 `GuildStore` 文件；频率低（管理操作），沿用 A6 控制邮箱的 stop-and-wait 可接受（`@` GM 命令同族）；
**不需要** A20 那种新非阻塞通道（那是给高频聊天 `!~` 用的）。

---

## 2. 写集与冲突点

### R1 `mir2-rs/bins/game-svr/src/lib.rs`（唯一要改的文件）

| 位置（符号锚） | 改动 | 与 **片 3（交易/组队）** 重叠 |
|---|---|---|
| `zone_cmd_class` 的 **Process 判定块**（现仅 `CM_CLICKNPC`/`CM_MERCHANTDLGSELECT`） | 追加 `guild_cmd::GUILD_IDENTS.contains(&ident) ⇒ ZoneCmdClass::Process`（ident 级、不依赖 payload） | **同函数、不同子块**：片 3 往 `ident_level` 链尾追加 10 条 Local；两者相隔数行，git 多半自动合并；若冲突 = **两边都保留**。 |
| `ControlPayload`（enum） | 新增 `GuildProcess { ident: u16, tail: Option<String> }` | 不重叠（片 3 不加 payload 变体） |
| `ControlRequest`（enum）+ `zone()/chr_name()/at_tick()` 三个 accessor 的 match | 新增 `GuildProcess { zone, chr_name, ident, tail, at_tick, epoch }` 并补三处 match 臂 | 不重叠 |
| `a6_enqueue_control`（`ControlPayload` → `ControlRequest` 的 match） | 新增 `GuildProcess` 臂（`zone/chr_name/at_tick/epoch` 同 `EatProcess` 形态） | 不重叠 |
| `a6_dispatch_local` 的 `ZoneCmdClass::Process` 分支（现按 ident 分 `EatProcess`/`ScriptProcess`/默认 `GmSay`） | 新增：`GUILD_IDENTS ⇒ ControlPayload::GuildProcess { ident, tail }`（**在默认 GmSay 之前**，否则会被当 `@` 行） | 片 3 的 `a6_dispatch_local` 不改（它们走 Local 臂）；**无直接重叠** |
| 区 0 控制邮箱 drain 的大 match（`ControlRequest::GmSay/EatProcess/ScriptProcess` 三臂处） | 新增 `ControlRequest::GuildProcess` 臂：此时已锁**目标区 k** 的 `ZoneState`（与 GmSay 同机制），用区 k 的 `world/displays/online/ground_sessions/gates` + 进程级 `guilds(&mut)/guild_cfg/guild_store/enter_cfg.guild_sections/enter_cfg.guild_msg_cfg/enter_cfg.server_index/colors/ref_msg_range` 调 `guild_cmd::handle_guild_cmd(...)`；`for ev in events { glog!("INF", "行会跨服事件：{}", ev.describe()) }`（与区 0 `INBOUND_PROCESS` 臂逐字一致） | 不重叠（片 3 不动控制邮箱） |
| （建议）新增共享助手 `run_guild_command(...)` | 区 0 `INBOUND_PROCESS` 的 guild 臂与 drain 臂**共用同一份**实参装配，防两处漂移；若不做，drain 臂直接抄同一组实参（风险：以后改签名两处漂） | 不重叠 |
| 测试模块（`zone_command_tests` 等） | 新增 §4 的牙；若片 3 也在同模块 append，**同模块 append 冲突**（解：两边都留） | **同测试模块**，append 冲突点 |

**不改**：`guild_cmd.rs`（9 条实现完备）、`guild_frames.rs`、`guild_runtime.rs`、`world-bevy/guild.rs`、
`zone_cmd_class` 的 Local 链、`handle_zone_command_on` 交易/组队臂（片 3 面）。

### 依赖/前置

1. **A35-b 已合**：区 k 有真实 `ground_sessions`/`online`/`world`（迁移随带）。
2. **A6 已有**：控制邮箱/stop-and-wait/超时/丢弃计数（片 4 只加一个变体）。
3. **A33/A20 不冲突**：104 广播属未来 `!~` 通道片，本片不产 104。
4. **片 3 合并序**：谁后合谁 merge master（不 rebase）；`zone_cmd_class` 冲突按「两边都保留」解，
   `zone_cmd_class` 最终清单交 Leader 核（片 3 的 10 条 Local + 片 4 的 9 条 Process）。

---

## 3. 帧扇出（谁收、recog 是谁、跨区怎么办）

| 命令 | 帧（ident） | 收件人 | recog |
|---|---|---|---|
| G1/G2 1035/1036 | `753`（面板）/`754`（失败） | **仅自己** | 自己 |
| G3 1037 | `756`（名册文本）/或 `753` 的同一面板路径 | **仅自己** | 自己 |
| G4 1038 | 直发 `757`/`758(nC)`；排队 `750`→`42`→`100`（绿） | 自己（757/758）；**目标**（750/42/100，目标按 `online` 找；离线 ⇒ 750/42/100 不产 + WRN） | 目标自己的 actor id |
| G5 1039 | 直发 `759`/`760(nC)`；取消支自己 `750(None)`+`42`+`100`（红）；被删目标 `750(None)`+`42` | 自己；被删目标（仅当 `online` 有） | 各收件人自己 |
| G6 1040 | `753`（open_dlg 面板） | **仅自己** | 自己 |
| G7 1041 | 失败 `761(nC)`；成功 **0 帧**（N3：ranks 清空 ⇒ 文本空） | 自己 | 自己 |
| G8 1044 | `769(n8)`（生产恒 `-4`；成功 `768` 不可达） | 自己 | 自己 |
| G9 1045 | `769(0)`（`771` 全仓无发点；`770` 不可达）；非掌门 **静默** | 自己 | 自己 |

**要点**：
- **`42`（RefShowName）是唯一多收件人帧**：收件人 = `ref_msg_cache::targets(world, target, ref_msg_range)`
  = **本区 world** 的 ±range 观察者（含自己），`Recog` = **被展示者**（不是收件人）；同一 body 可复用。
- **`SM_GUILDMESSAGE(104)` 不由这 9 条产生**（104 是行会聊天/公告广播 `guild_runtime::guild_broadcast_104`，
  属 A20 `!~` 后续通道片；届时必须**逐人构造**，`recog=收件人自己`——A33 教训）。
- **跨区扇出 = 本片不做**，依据：
  1. 收件人解析全部走**执行区自己的** `online`/`world`（`find_play_object` 按 `OnlineRegistry` 查名，
     `send_to` 按 `online.find_linear_actor`，`ref_show_name` 按本区 world）；别区玩家/镜像不在本区 `online`
     ⇒ 帧被丢弃（现状只剩 WRN）。
  2. `cross_zone_frames` 只承载**镜像收件人**的 RefMsg 面（`(src_zone,tick,seq)` 去重 + 目标区邮箱），
     与 `SendDefMessage` 的「按登记表直发某 socket」不是同一路径；硬套会改帧面语义。
  3. 区 0 直发方案（A20 设计 §2.3 的「逐区加锁读 online」）能做，但那是**独立决策**（一次只持一区锁、
     锁序、帧从区 0 发），且本片 9 条里 **7 条只回自己**，收益低；建议登记为缺口，留给「跨区行会」卡。

---

## 4. 牙草案（每族三态：同区成功 / 目标不在区 / 名册跨区）

统一夹具：`ZoneState::new(zone_k, …)` + `SharedServices`（注入 `catalog`/`guild_sections`/`guild_msg_cfg`）+
进程级 `GuildManager`/`GuildConfig`/`GuildStore`（临时目录）；`gates` 单通道；玩家 A（掌门）/B（同区成员）/
C（他区玩家：本区 world 里 `handoff_accept(MirrorSpec)`、**无 online 项**）。

1. **G1/G2 同区 + 无会**：A 无会 ⇒ 恰 `754`；入会后 ⇒ `753`（面板文本逐字节）。
   变异：不发 754/753 ⇒ 红。
2. **G3 名册文本**：A 会内有 A/B，B 离线 ⇒ `756` 仍按名册列 B（oracle 按名册拼）；断言文本字节。
3. **G4 同区成功**：A 掌门、B `allow_guild=true` 且面朝 A ⇒ A 收 `757`、B 收 `750`+`42`+`100`（绿）；
   `GuildManager` 名册新增 B、`GuildStore` 临时文件被写、B 会话 `guild_name` 置位；
   `zone_cmd_unsupported` **不增**；控制邮箱 `enqueued/drained` 各 +1、`control_pending` 归 0。
   变异：不写 B 会话 ⇒ 红；把 Process 改回 Unsupported ⇒ 红（计数变化 + 零帧）。
4. **G4 目标不在区**：B 只以镜像存在（无 online）⇒ `758/2`（oracle `GetPlayObject == null` 同形）；
   断言镜像实体/会话**未被写**、无 42/100 发往镜像。
   变异：去掉 `find_play_object` 的 online 依赖改扫 world ⇒ 命中镜像 ⇒ 红。
5. **G5 同区成功 + 自删**：A 删 B ⇒ `759` + B `750/42`、名册移除；A 自删（单人会）⇒ `CancelGuld` 成功
   ⇒ `759` + 自己 `750/42/100`(红) + `del_guild` + 落列表。
   变异：自删不调 `CancelGuld` ⇒ nC=3 帧 760 ⇒ 红。
6. **G6/G7**：1040 更新 notices 只回自己 `753`；1041 成功 0 帧、失败 `761`。
7. **G8/G9 生产形态**：1044 ⇒ `769/-4`；1045 掌门 ⇒ `769/0`、非掌门 ⇒ 零帧。
8. **名册跨区（缺口牙）**：A 掌门（区 k），名册里另有 C（区 0，不在本区 online）。A 做 G4 对 C 名：
   目标解析走 online ⇒ 视为「不在线」⇒ `758/2`（**与 oracle 单世界不同**，本片登记）且**不写 C**；
   断言本区计数/帧无异常、无 panic。
   变异（防倒退）：把 `find_play_object` 改成跨区扫（若将来实现）须另配新牙并改本条期望——**不得**在本片放宽。
9. **控制邮箱守恒**：`a6_mailbox_full_warn` 路径：预填 `CONTROL_MAILBOX_CAP` ⇒ 新 guild 命令被丢 +
   `zonek_control_mailbox_rejected` +1、`zone_cmd_unsupported` 不增（这是 Process 族正确归属的证据）。

---

## 5. 已知限制（原句进卡残余）

> **行会 9 条本片只做「同区可执行」，跨区名册成员的帧不投递。**
> - 区 k 执行时收件人解析只用**本区** `online`/`world`；名册里其他区的成员会被当作「离线」
>   （`GetPlayObject == null` 同形）⇒ 750/42/100 不产、`42` 观察者只覆盖本区。
> - 不建行会跨区名册同步、不做行会管理命令跨区、不做 `@全体`/系统公告。
> - 走 A6 控制邮箱 ⇒ 命令执行遵守 stop-and-wait（该玩家本地命令最多暂缓 1 tick、可进
>   `deferred_local`），与 `@` GM 命令同族；**不**新增非阻塞通道（那是 A20 `!~` 聊天片的选项）。
> - 不产出 `SM_GUILDMESSAGE(104)`（104 属 `!~`/公告广播，未来通道片；届时逐人构造 `recog=收件人`）。

---

## 6. 裁定（opc-27 2026-11-24，两点都批）

1. **抽 `run_guild_command(...)` 共享助手**：区 0 `INBOUND_PROCESS` 臂改为调它，行为/日志**逐字不变**
   （片 4 落地后以 N=1 层① 复核）；drain 臂同调。**不做两份逐字同形**。
2. **跨区投递 = 本片不做、登记缺口**（理由见 §3）。残余原句（进卡）：
   > **G4/G5 对"人在别区"的成员会表现为离线失败（`758/2`）**——区界对玩家不可见，这是**可见差异**，
   > 登记而不是含糊；
   > **`42` 的观察者只覆盖本区**，别区观察者看不到。
   > 「跨区行会」作为后续卡，与 A18/A20 的通道片一并考虑。

**实施排队**：等 opc-27 发「片 4 开工 = master `<sha>`」（片 3 合并后的 head；**只并一次**）；届时按 §2
写集实施，`zone_cmd_class` 冲突按「两边都保留」（片 3 的 Local 链尾 + 片 4 的 Process 块），并贴最终清单。
