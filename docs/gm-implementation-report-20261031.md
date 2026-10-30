# GM 65 条缺口实现收口报告（2026-10-31）

- 卡：`openmir2-gm-implementation-report`（tier=L，worker=openmir2-dev-1，分支 `card/gm-report-dev1`）
- 基线：`walgit/master` `5ef698f0a`（W 族合并提交）；写集：仅本文档
- 判据来源：`docs/gm-unimpl-inventory-20261030.md`（74 条钉性盘点）、`docs/gm-registry-rebuild-20261031.md`（登记表重建）、九族卡线程与真机台账 `openmir2-real-machine-verification-master`
- 口径：行为等价优先于修 bug；oracle 源码里已被注释或不可达的面按原样登记，不补实现、不作「修 bug」夹带

## 1. 结论

**74 条 `Imp::Unimplemented` 盘点 → 65 条真缺口全部实现（9 族）+ 9 条已对齐 + 0 条垃圾命令。**

- 65 条按族拆为 9 张族卡：P 7 / T 7 / Q 10 / X1 5 / X2 18 / F 3 / M 6 / C 5 / W 4（7+7+10+5+18+3+6+5+4 = 65）。
- 9 条「已对齐」在盘点时即判定为 oracle 过权限门后恒零帧（体注释 / 空体 / 死分支 / 反射异常被吞），按登记口径换成 `no_frames` + `log_note`，等价零帧不另造帧。
  - 成员：`ClearItemMap` / `EndContest` / `ServerInfo` / `SetMapMode` / `showopen` / `showunit` / `Total` / `who` / `查看周围`。
- 0 条垃圾命令：卡面假设的 `fdgfdg56 / fdht7ed / gfh678dfg / ghjh6 / hjhg556 / hjhgj67 / sadsa54 / sdasdert / s` 全部是 `Command.conf` 的混淆现网名，各映射到一个真实 oracle 类（详见盘点 §3）。
- tier 终态：L 5 族 32 条（P/Q/T/X1/F）+ R 4 族 33 条（M/C/W/X2）；F 初判 R，审查中以「quest 不落库」证据降为 L（§4）。
- 追加收口：X1 审查暴露的「源文本建模 vs 运行层真相」盲区，由扫盲卡 `openmir2-gm-systemshare-null-sweep` 系统化处理，修 **8 条**既有命令的运行层帧面偏差（§3）。
- 最终状态：master `5ef698f0a`；合并树全量门禁 `RAW_RC=0`、**3463 passed**；registry gate **GREEN 9 条**（= 余 9 条已对齐）；R 级真机判据在台账中的**已登记项全部索引**（§5；X2 条目 `be337381` 由 orchestrator 补签）。

## 2. 九族总表

merge oid 为各族 `merge_result` 记录的合并提交（均为 master `5ef698f0a` 的祖先）；review 为最终 `approve`（前序 `request_changes` 一并列出）。「变异数」口径 = **最终复审条目记录的重放 + 自造变异数**（排除纯探针）；X2 另列作者清单，W 按复审 m2 订正为 12。

| 族 | 条数 | 卡名 | merge oid | review（reviewer + oid） | 变异数 | 关键判决 |
|---|---:|---|---|---|---:|---|
| P 密码锁门 | 7 | `openmir2-gm-family-p-passwordlock` | `e16572145` | reviewer-20 `29475a0a` approve（L） | 4 | 部署 `PasswordLockSystem=0` ⇒ 恒红「游戏密码保护系统还没有启用!!!」+ 零副作用；RM_PASSWORD 状态机现网不可达，不实现 |
| T 会话开关/骑马 | 7 | `openmir2-gm-family-t-toggle-horse` | `0bdf71ad4` | reviewer-21 `e240dbec` approve（L） | 5 | 四条 toggle 双绿字往返；`OffHorse`/`OnHorse` 的 `SM_FEATURECHANGED(41)` 外观帧宿主落地；`DisableFilter` 翻全局 `FilterWord`（Cell 注入） |
| Q 查询/展示 | 10 | `openmir2-gm-family-q-query-display` | `3fb67e16a` | reviewer-20 `9590cb87` approve（L） | 4 | 四怪癖逐条照抄：IP 判空恒真恒红帮助 / 信息在线静默 / Map 缺参越界零帧 / Showuseiteminfo 单字符名越界零帧 + `%s` 格式串字面输出 |
| X1 禁言名单 | 5 | `openmir2-gm-family-x1-denywhisper` | `5e608999f` | reviewer-21 `493c571f` rc → `e6163c0c` approve（L） | 3 | 运行层真相：`SystemShare.DisableSendMsgList/DenySayMsgList` 声明即 null、全仓零赋值 ⇒ 触表四命令体真机 NRE 被吞 = **零帧**（源文本红/绿分支不可达） |
| X2 目标控制/杂项 | 18 | `openmir2-gm-family-x2-target-control` | 批 A `92c352ff9` + 批 B `1115bd1ef` | 批 A reviewer-21 `3400dd83` rc → `65ec7767` approve；批 B reviewer-20 `b6a3962b`/`d7a57cd0`/`27f360af` rc → `cb7519e2` approve（R） | A: 4 重放 + 3 取证／B: S1/S2 + M-T3/M-T8（作者清单 M-T1..M-T8） | 真副作用写面：`vv杀死`/`复活`/`VV给予权限`/`删除技能`/`ClearCopyItem`/`清除仓库密码`；`复活` 的 `SM_ABILITY` 投被复活者本人 + 清 `death_settled` + `SM_ALIVE` 广播 |
| F 脚本标志 | 3 | `openmir2-gm-family-f-script-flags` | `98979d829` | reviewer-21 `acd08f07` approve（**R→L 降级**） | 3 | `setflag/setopen/setunit` 写**调用者**位阵（非 target）+ 对目标回绿字；runtime 真身 `saves.record.data.quest_*` 会话内不落库 ⇒ 落库前提证空 |
| M 刷怪/造物 | 6 | `openmir2-gm-family-m-spawn-make` | `e79d754ba` | reviewer-21 `e0063862` rc → `8cb70f04` approve（R） | 5 | `GiveMine Random(18)+3` 恰一次、Make 每件 `Random(100)`；`Length>0` 守卫越界零帧怪癖照抄；`@怪` 三族 `ApplySlaveLevelAbilitys` recalc（I1）；MobPlace 偏登记 T-271 |
| C 城堡/赛事 | 5 | `openmir2-gm-family-c-castle` | `a0448b17b` | reviewer-20 `0d2fd0f5` rc → `f555c9f6` approve（R） | 3（复审重放；作者 patch 另称 10） | 三处源码文本不可达（SabukWallGold 无参 / 沙城 NotImpl / StartContest 零 cry）；开战帧序 42→21→100 + 主门 `SetMapXyFlag` 12 op 当场落地（I1） |
| W 行会 | 4 | `openmir2-gm-family-w-guild` | `5ef698f0a` | reviewer-21 `72841551` rc → `4e25d4cc` approve（R） | 12（复审 m2 订正，原 9） | `AllowGroupReCall` 是**独立**字段（I1）；`characters.AllowGroupReCall` 列 bind 抄 oracle bug-for-bug（T-272）；EndGuild 帧序 750→42→100 |

> 合并链（按合并时间序，中间省略非 GM 族合并如 `365f27349`/`6638fae46`/`907f8c09e`）：`e16572145 → 0bdf71ad4 → 3fb67e16a → 5e608999f → 3a9ae1203(扫盲) → 92c352ff9 → 1115bd1ef → 98979d829 → 53973b7e9(registry 重建) → e79d754ba → a0448b17b → 5ef698f0a`。

## 3. 扫盲：SystemShare null 表与 8 条运行层偏差

卡 `openmir2-gm-systemshare-null-sweep`（tier=R），merge `3a9ae1203`，reviewer-21 `ac8c3ab2` rc → `602be6b2` approve。目标不是补 65 条缺口，而是回扫**已实现**命令：找出「rust 按源文本发帧、oracle 真机 NRE 零帧」的帧面偏差。

### 3.1 A1 字段穷举

- `SystemShare.cs` 声明即 `null` 共 **34 处**；其中 **3 个有赋值**（`ManageNPC`/`RobotNPC`/`FunctionNPC`，各 4 处），**31 个 NULL-FOREVER**（全仓零赋值）。31 个中 **11 个有读点**、**20 个零读点**。
- 有读点的 11 个：`DenySayMsgList`、`SellOffItemList`、`DynamicVarList`、`DisableSendMsgList`、`ItemBindIPaddr/ItemBindAccount/ItemBindChrName/ItemBindDieNoDropName`、`DenyIPAddrList/DenyChrNameList/DenyAccountList`。
- 计数订正：初版 worker 注释写「29 NULL-FOREVER」，review M2 按实况订正为 **31/3/20**（34 = 3 有赋值 + 31 NULL-FOREVER；31 = 11 有读点 + 20 零读点）。

### 3.2 A2 偏差清单（8 条全部零帧化）

| # | 命令 | oracle 运行层真相 | 修复 |
|---|---|---|---|
| 1 | `@禁言`（ShutupCommand） | `EnterCriticalSection(DenySayMsgList)` 触 null，早于 SysMsg ⇒ NRE 被外层 catch 吞 ⇒ 零帧；校验失败红帮助在其前可达 | 红字支 → 零帧 + `NOTE_SYSTEMSHARE_NULL` |
| 2 | `@-jid`（DelDenyAccountLogon） | for-init 读 `DenyAccountList` ⇒ NRE ⇒ 零帧；空名红帮助在前可达 | 绿支 → 零帧 + NOTE |
| 3 | `@DelDenyChrNameLogon` | 同构（`DenyChrNameList`） | 同上 |
| 4 | `@-jip`（DelDenyIPaddrLogon） | 倒序 for-init 同构（`DenyIPAddrList`） | 同上 |
| 5 | `@+-jid`（ShowDenyAccountLogon） | `Count` 读 null ⇒ 零帧；`perm<6` 内门保留 | 绿支 → 零帧 + NOTE |
| 6 | `@ShowDenyCharNameLogon`（现网活名；类属性名 `ShowDenyChrNameLogon`） | 同构（`DenyChrNameList`，无内门） | 同上 |
| 7 | `@+-jip`（ShowDenyIPaddrLogon） | 同构（`DenyIPAddrList`） | 同上 |
| 8 | `@Binduseitem`（BindUseItemCommand） | 合法槽名可达：离线红 / 在线空槽红 / 在线已穿戴触 `SystemShare.ItemBind*`（null）⇒ NRE 零帧 | 落 13 名常量表 + 三支（review F1 补修） |

- 第 8 条由 reviewer-21 F1 追加：`SystemShare.GetUseItemIdx` 在 oracle 是编译期常量表（13 个 `MessageSettings` const 槽名），无宿主依赖 ⇒ 合法槽名输入类真实可达，是扫盲目标模式；首轮「无偏差」判定被证伪。
- 无偏差项：`@取消禁言`（`A || !A` 恒真早退在 `Enter` 之前 ⇒ 帮助文本可达，rust 一致）。
- A2 修复不扩写集到「不可达支补实现」：触及 null 表的命令体统一零帧 + log_note，可达红帮助原样保留。

## 4. 关键裁定汇总

逐条一行为主；证据引用路径 + 符号（不写行号锚）。

- **F tier R→L**：oracle quest 数组是会话内 `CharacterData`，`SAVE_CHARACTERS_SQL` 无 quest 列，且 `mir2-rs/bins/db-srv/tests/quest_no_persist.rs` 已把「不落库」固化成门禁 ⇒ ③ 持久化前提证空，无 RNG/时序/战斗面，降为 L。
- **F 写调用者怪癖**：`SetFlagCommand/SetOpenCommand/SetUnitCommand` 调 `PlayerActor.SetQuest*Status`（调用者），**不是** `targetObject`；对目标回 `{ChrName}: [n] = ON/OFF`（按写后读回判定）；target 离线 / 缺参的 NRE 被 `PlayObject.Chat.cs` 外层 catch 吞 ⇒ 零帧零写。
- **C 三处源码文本不可达**：`ShowSbkGoldCommand` 无参支 `CastleManager.GetCastleGoldInfo` 的 `List.Add` 整段注释 ⇒ list 恒空 ⇒ 零帧；`ChangeSabukLordCommand` 的 `castle.GetCastle(guild)` 静态类型是 `IUserCastle`，其 `GetCastle(IGuild)` 在 `UserCastle` 是 `throw new NotImplementedException()` ⇒ 零帧无状态改；`StartContestCommand` 的 `CryCry` 对 `SendMsg(null, RM_CRY, …)` 解引用 null `ActorId` ⇒ NRE 中止 ⇒ 零 cry 帧。
- **C I1 主门掩码**：`@开始攻城` 的 `main_door_control` 产生的 12 条 `SetMapXyFlag(1)` 必须**当场落到该城地图**；只喂 `queue_structure_ref_msg` 会丢门格掩码，主门仍可走且不自愈。帧序按 oracle MsgQueue：`SM_USERNAME(42)` → `SM_DIGDOWN(21)` → `SM_SYSMESSAGE(100)`。
- **C 怪癖**：`SabukWallGold` 的 `-` 恒减 1（忽略 nGold）、`TotalGold<0` 钳 0；`char ctr = sCtr[1]` 使第二参不足 2 个 UTF-16 码元越界零帧；`StartContest` 的 `!ObMode || !AdminMode` 是**或**语义，照抄。
- **W I1 独立字段**：`@允许天地合一` 翻的是 `PlayObject.AllowGroupReCall`，与 `@sadasd` 的 `AllowGuildReCall` 是**两个字段**；首翻值取 `characters.AllowGroupReCall` 登录值，并写 `Entity.allow_group_re_call`（`@天地合一` 成员门）。
- **W I2 / T-272 bug-for-bug**：oracle `PlayDataStorage.Save.cs` 把 `@AllowGroupReCall` 参数绑到 `hd.AllowGroup`，`AllowGroupReCall` 字段从不进 SQL；rust `playdata.rs` 照抄同一 bind ⇒ DB 列不随命令变化属 **oracle 原样**，不以列 diff 为锚；`build_save_record` 未回写 live `AllowGroup` 是既有偏差，另卡另裁。
- **652 双层编码（A1 七层链）**：oracle 尾 = `EncodeString( EncodeBuffer(item1)+"/"+… )`（`Merchant.ClientGetDetailGoodsList` 内层逐件 + `PlayObject.Socket.SendDefMessage` 外层整串）；冻结客户端 `ClMain.pas` 的两遍解码正是期望形态 ⇒ rust 现状逐字节同构，**不改线缆**。注：`651 param=0` 真因是服务端 `ClientItem` 124B 与客户端 `TClientStdItem` 116B（`TClientItem` 总 124B，`MakeIndex@116`）的口径差（`MakeIndex` 读到服务端 `Desc` 全零区），非编码层。见 `docs/gm-652-encoding-layer-a1-20261030.md`。
- **X1 四命令体零帧（源级证伪）**：`SystemShare.DisableSendMsgList/DenySayMsgList` 声明即 null、全仓零赋值；`+jy` 非空支 / `-jy` 非空支 / `+-jy` / `查看禁言` 触表即 NRE 被 `PlayObject.Chat.cs` 顶层 catch 吞 ⇒ 零帧。`+jy` 在线目标置 `FilterSendMsg=true` 是 NRE 前真实副作用（非帧，note 记语义）。
- **M I1 三族 recalc**：`@怪` 召出 WhiteSkeleton / ElfMonster / ElfWarriormon 须调 `ApplySlaveLevelAbilitys`；公式按 oracle `DC = round(HiByte + 3*(0.3+exp*0.1)*exp)`、`MaxHP = Abil.MaxHP + round(Abil.MaxHP*(0.3+exp*0.1))*exp`。
- **M RNG 站点**：`GiveMine` 纯度未填 ⇒ `Random(18)+3` **恰一次**；`Make` 每件 `Random(MakeRandomAddValue)` 一次；宿主以唯一标记登记 RNG site，牙用 `rng_trace` 按 `file:line` 断言次数与参数（代码注释内，报告不复制行号）。
- **M T-271**：`@MobPlace` 刷出的怪未置 `Mission/MissionX/MissionY`；宿主 `crates/world` 的 `Entity` 无该三元字段、怪 AI 无消费点 ⇒ 只按数量 `RegenMonsterByName`。生产不可达（perm 10 + 部署脚本无 GMEXECUTE），按白名单 T-271 登记。
- **X2 复活**：`SM_ABILITY(52)` 投**被复活者本人** socket（oracle 收件人自投）；清 `death_settled`、`death=false`、`HP=MaxHP`、同步会话 `wabil.hp/max_hp`；`SM_ALIVE(27)` 视距广播；双绿字 ASCII 句点 vs 全角句号怪癖照抄。
- **X2 删除技能**：`MagicList = saves.record.data.magic` 真实实现；`SendDelMagic` 投目标 + 双方绿字 + `recalc_session_ability`；`All` 清空不发帧不发字；审计 `SendSocket` 直发帧必须排在两条绿字之前（oracle SendSocket 立即、SysMsg 入队）。
- **X2 ClearCopyItem 怪癖**：内层删除写的是调用者 `PlayerActor.ItemList` 而非目标 ⇒ 1:1 照抄；调用者==目标时活表迭代（oracle 删正在遍历的同一张表），非自目标快照等价，越界异常停止，两路零 SysMsg。
- **Q 展示怪癖**：`Showuseiteminfo` 的格式串是 `%s[%s]IDX[%d]…`，.NET `string.Format` 无 `{n}` 占位 ⇒ **原样字面输出**；判空用 `sHumanName[1]` ⇒ 单字符名越界零帧。`HumanLocalCommand` 判空写反成 `IsNullOrEmpty(s) || !IsNullOrEmpty(s)` 恒真 ⇒ 恒红帮助。
- **P 共享门死支**：`PasswordLockCommand` 独有的静默支被上提到 7 条共享门；两侧恒假（空串 `Split(' ')` 长 1），行为等价，注释自报。
- **T 骑马**：`TakeOnHorseCommand` 的 `HorseType=0` 红「骑马必须先戴上马牌!!!」；上马 + `FeatureChanged()` 外观帧 + `FunctionNPC.GotoLable("@OnHorse")`（宿主接缝，`FunctionNPC` 为空时 NRE 被吞，登记零帧）；`TakeOffHorseCommand` 未骑马静默。
- **M 刷怪三支**：`MobNpc` 缺参/`?` 红帮助、足参**静默**（建 NPC 体整段注释）；`MobPlace` Mission 未设或图不存在 ⇒ 两条红，已设 ⇒ `_MIN(500,n)` 只刷怪；`ReCallMobEx` 实参 <4 越界零帧，足参 ⇒ 刷一只 + `Master=调用者` + `SlaveExpLevel=1` + `NameColor` 钳 [0,255] + `RecalcAbilitys` + `RefNameColor(656)`，**不置 IsSlave / 不补 HP**。
- **X2 探测**：`ProbeNecklace || Permission>=6`（外门已 10 ⇒ 恒过照抄）；10 秒节流（perm>=3 免）+ `ProbeTick` 写；在线蓝字定位尾坐标 y 用**调用者** `CurrY` 怪癖。
- **X2 脚本接缝**：`MemberFuncEx` / `是我` 的 `GotoLable("@Member")` 与 `ScriptGotoCount=0` 走宿主接缝 `log_note`（脚本桥 `GmWorld` 未建模时退化零帧）。
- **C 争霸赛范围**：`StartContest` 范围 1000 为**闭区间**，`!Death && !Ghost` 且在 `!ObMode || !AdminMode` 时 `FightZoneDieCount=0`；`Fight3Zone` 门红字。
- **W 行会文件**：`AddGuild` 调 `GuildMgr.AddGuild` + `GuildConf` 建 `.ini` + `SaveGuildInfoFile`（`save_one`）；缺 `<名>.txt` 时记「保存行会信息失败!!!」日志、**不阻断**，sibling `guild_official` 同法；`SaveGuildList` 后 `SS_205`。
- **扫盲第 8 条**：`@Binduseitem` 13 名常量表逐名逐值对齐 `SystemShare.GetUseItemIdx` / `MessageSettings`；门后三支（离线红 / 空槽红 / 已穿戴零帧）与 `BindUseItemCommand` 逐行对齐。

## 5. 真机台账 V-* 索引

台账线程：`openmir2-real-machine-verification-master`。每条一行；窗口核验时按原始帧日志（`frames.ndjson`）为准。

| 项 | 来源卡 / merge | 真机判据（窗口） |
|---|---|---|
| SW-1..SW-6 | `openmir2-gm-systemshare-null-sweep` / `3a9ae1203` | SW-1 `@禁言` 在线已禁言 → 零帧；SW-2 `@-jid` 非空 → 零帧；SW-3 `@DelDenyChrNameLogon`/`@-jip` → 零帧；SW-4 `@+-jid`/`@ShowDenyCharNameLogon`/`@+-jip` → 零帧；SW-6 `@Binduseitem` 在线已穿戴 → 零帧（另核离线红、空槽红可达）；**SW-5 已移出**（属 X1/F3） |
| V-m-spawn | `openmir2-gm-family-m-spawn-make` / `e79d754ba` | GiveMine `Random(18)+3` 与 Make 每件 `Random(MakeRandomAddValue)` 的 site/次数/参数同流位序；`GetItemNumber/Ex` + `GetTickCount` 未建模的 `SM_ADDITEM` 字节差；实体生成 + `SM_ADDITEM` 帧；`@怪` 三族 recalc DC/MaxHP |
| V-c-castle | `openmir2-gm-family-c-castle` / `a0448b17b` | `TotalGold` 落盘（`=`/`-`/`+` + 钳 0）；开战 42→21→100 帧序 + 主门格可走性（12 条 `SetMapXyFlag` 落地）+ 再发停战；三处不可达复核；`FightZoneDieCount` 落库 + 零 cry；`ShowPreFixMsg=0` 前提（T-224 口径） |
| V-w-guild | `openmir2-gm-family-w-guild` / `5ef698f0a` | AddGuild + `SS_205` / DelGuild `SS_206` / EndGuild `SS_207` + 750→42→100 帧序；`@允许天地合一` 内存切换（DB 列不变 = T-272 oracle 原样） |
| V-x2-target-control | `openmir2-gm-family-x2-target-control` / `1115bd1ef` | Kill 延迟时序（`SM_DEATH` 下 tick）/ 复活帧方向 + `HP` + `SM_ALIVE(27)` / 删除技能 `212` + 双绿字 + 审计帧序 / `ClearCopyItem` 活表怪癖 / `DelSkill RecalcAbilitys` 审计 / 探测 10s 节流 + 尾坐标 `CurrY` / `Reconnection` `BoReconnection` 置位（台账条目 `be337381`） |
| V-659（`V-groupmode-659-g2`） | 已合并的 659 调查链 | 已收口：dev-4 双连接 12 轮 659 到达率 12/12、param 全符、glog 零丢帧 ⇒ rust 无缺口；历史 2/6 = 旧二进制 + 采样竞读 |
| V-81（`V-querybagitems-81`） | `openmir2-querybagitems-81-throttle` A3 | 已确认：aaa 空包 81 零回包（`IsNullOrEmpty` 守卫）+ 30s 节流；probe `smoke-81-obs` 互证 |
| V-dealtry（`V-dealtry-e2e`） | `openmir2-dealtry-adjacent-resolution` A3 | 已收口：对角钉位空名 `CM_DEALTRY(1025)` 双端 673 到达（两轮复现）；oracle `GetPoseCreate` 无相邻扫描，背向/空前排零回音 = oracle NRE 吞行为 |
| V-buy-stock | `openmir2-gm-buy-stock-instances` / `6638fae46` | 已转「oracle 原样行为登记」：652 线缆双层不改；651 = 客户端 `TClientStdItem` 116B（`TClientItem` 总 124B，`MakeIndex@116`）cast 读 124B 帧错位，C# 旧栈同一份字节同样 651 ⇒ 服务端无差；武器类明细购买不可达 = 白名单项，购买链验收改用 StdMode≤4 药品商人 |

## 6. 残余登记（一处一行，非阻塞）

- **T-271**：`@MobPlace` 的 `Mission/MissionX/MissionY` 三元未建模 + AI 无消费点；生产不可达，接线前不得视为等价（`mir2-rs/tests/parity/whitelist.md`）。
- **T-272**：`characters.AllowGroupReCall` 列 bind 抄 oracle bug-for-bug；`build_save_record` 未回写 live `AllowGroup` 的既有偏差需另卡（`mir2-rs/tests/parity/whitelist.md`）。
- **m3（W 注释旧口径）**：`gmcmd.rs`/`lib.rs` 里「存档写 `characters.AllowGroupReCall`」类注释应改为「写 `PlayerRecordData.allow_group_re_call`（DB 列绑定见 T-272）」；随下一次触碰该文件时统一。
- **m5（扫盲牙强度）**：`@Binduseitem` 13 名槽位只钉住「项链」索引值 3，其余 12 个索引值改错无用例发现（自造变异 M-R21-3 存活）；随窗口批次补。
- **N1（X2 flame_ring 正支）**：`flame_ring_in=true` 的粘滞落账与 `revival=true` 写回无正向断言；实现与其它 9 处逐点一致，属覆盖缺口（reviewer-20 `cb7519e2`）。
- **N2（X2 断言掩蔽）**：主案例「只删命中项」的列表断言会被随后审计删除掩蔽；真正咬住 S2 的是帧序断言（reviewer-20 `cb7519e2`）。
- **X2 R1/R2/R3**：`CM_SAY` 排水门 `trim_start` 宽于 execute 门；`@Login` 第 7 站仍用 `GmConfig::default()`（部署值等于默认 ⇒ 冻结零差）；F3 顺序覆盖面缺口（实现顺序正确）。
- **F4（registry 重建文档字节数）**：`docs/gm-registry-rebuild-20261031.md` 摘要写 `99269 bytes`，实跑 `WROTE 97262 bytes`；docs-only 后续订正，构建可逐字节复现不受影响。
- **Q provenance**：`LotteryTicket` 八配置「部署无此键」不实——部署 `Server.conf` 显式八键全 `=0`，ctor 清零先于 conf 加载；结论「部署全 0」不变。
- **P info**：`PasswordLock` 独有静默支被共享给 7 条（两侧恒假，行为等价，注释自报）。

## 7. registry gate 演进（RED 32 → GREEN）

`coverage/gm_registry.py`（`mir2-rs/tests/parity/coverage/`）把 `gmcmd.rs` 表项与 `gm_registry.json` 逐字段核对；随族合并递减：

| 阶段 | gate 状态 | 触发 |
|---|---|---|
| F 族合并后（重建前） | **RED(32)**：`table_total 183≠185` + 登记表陈旧 + 3 drift 伪差 + 2 命令名漂移 | 旧 parser 漏两条夹注释表项；登记表停留 `111/74` |
| `openmir2-gm-registry-rebuild` 合并 `53973b7e9` | **GREEN 24 条**（`table_total=185`、`161/24`、crosscheck 185/185） | parser 允许表项字段间注释 + `build` 重建 + 名字回 oracle 活名 |
| M 族 `e79d754ba` | GREEN **18 条** | 6 条刷怪/造物落地，登记表重建 |
| C 族 `a0448b17b` | GREEN **13 条** | 5 条城堡赛事落地，登记表重建 |
| W 族 `5ef698f0a` | GREEN **9 条** | 4 条行会落地；余 9 = 已对齐项 |

- 四因子（重建卡定性）：① parser 正则要求 `CommandDef {` 后紧跟 `name:`，漏掉夹注释的两条；② 登记表陈旧（旧 `111/74`）；③ `ClearItemMap/ContestPoint/EndContest` 的「drift」是按位置 `zip` 的错配，非字段真变；④ `f40303917` 把两条命令名改成类属性名拼写（`ChangeMasterName`/`ShowDenyChrNameLogon`），重建卡回 oracle 活名 `MasterName`/`ShowDenyCharNameLogon`。
- 门禁实跑（重建卡）：`build` rc=0 可逐字节复现、`gate` rc=0、`selftest` rc=0（17 牙，K/N 显式 skip）；最终 master 上 gate GREEN 9 条。
- 判据边界：registry 的 `unreachable` 只证「冻结部署无 GMEXECUTE 命中」，不等于行为等价；R 级真机项仍按 §5 台账核。
