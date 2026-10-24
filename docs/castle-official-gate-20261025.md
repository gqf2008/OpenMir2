# CastleOfficial 门（T-40/T-140）：Click / UserSelect 两道门 + 城堡说词变量（tier R）

- 卡：`openmir2-castle-official-design`（本片 = 该设计卡的**最小实现片**，owner `openmir2-dev-2`，
  branch `card/castle-official-door`）。
- 基线：`master = f08496192`（2026-10-25，含 CLEARMAPMON 合并）。
- 写集：`mir2-rs/bins/game-svr/src/{npc.rs,castle.rs,lib.rs,castle/tests.rs}`、
  `mir2-rs/crates/script/src/engine/mod.rs`（`ScriptPlayer::take_say_abort` + 3 处说词点）、本报告。
- **未改**：`src/**`（C# oracle）、`sql/`、`MirClient`、协议号表、场景 JSON、`E:/MirServer/**`。
- tier **R**：`$CASTLEGETDAYS`/`$DATETIME` 是墙钟、修门 60s 门与帧序属时序 ⇒ 真机窗口复核；
  合并后保持 in-progress。

## 1. oracle 锚（逐条）

| 主题 | 位置 | 语义 |
|---|---|---|
| `Castle` 归属 | `NormNpc.cs:1674-1678`（`Initialize`） | `Castle = CastleMgr.InCastleWarArea(this)`：NPC 所在 (map,x,y) 落在某城战区 ⇒ 该城；否则 `null`。部署 `Npcs.txt` 的 `沙巴克城堡管理 … 0150 …` 命中 `SabukW[Defense] CastlePlaceMap=0150` ⇒ 非 null |
| `Click` 门 | `CastleOfficial.cs:20-31` | `Castle == null` ⇒ `SysMsg("NPC不属于城堡!!!", MsgColor.Red, MsgType.Hint)` 后 return；否则 `Castle.IsMasterGuild(MyGuild) \|\| Permission >= 3` 才 `base.Click`；两条都不满足 ⇒ **零帧零日志** |
| `UserSelect` 顺序 | `CastleOfficial.cs:93-107` | **先** `base.UserSelect`（= `Merchant.UserSelect`：`GotoLable` + 脚本 `#ACT` + `@exit/@back` 账本）；**之后**才进 Castle 的 `try`（`:98`）：`Castle == null` ⇒ 同一红字；`@` 开头且 `IsMasterGuild && IsGuildMaster` 才走动作 switch（部署 R-0 恒 false） |
| 变量覆写 | `CastleOfficial.cs:33-91` | **先 `base.GetVariableText`**，再 `if (Castle == null) { sMsg = "????"; return; }`（**整行覆盖**），再 8 个 token：`$CASTLEGOLD`/`$TODAYINCOME`/`$CASTLEDOORSTATE`/`$REPAIRDOORGOLD`/`$REPAIRWALLGOLD`/`$GUARDFEE`/`$ARCHERFEE`/`$GUARDRULE` |
| `$CASTLEDOORSTATE` | `CastleOfficial.cs:52-69` | `(CastleDoor)Castle.MainDoor.BaseObject` —— **无 null 检查**；`Death⇒destroyed`、`IsOpened⇒opened`、否则 `closed` |
| `$DATETIME` | `NormNpc.cs:356-359` | `DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss")`（墙钟，展开时刻读） |
| `$CASTLEWARDATE` | `NormNpc.cs:805-831` | `Castle==null ⇒ Castle=GetCastle(0)`；`!UnderWar`：`GetWarDate()` 非空 ⇒ 替换 token，**空 ⇒ 整行**换 `"暂时没有行会攻城 .\\ \\<返回/@main>"`；`UnderWar` ⇒ 整行 `"正在攻城当中..."` |
| `$LISTOFWAR` | `NormNpc.cs:832-877` | 非空 ⇒ 替换 token；**空 ⇒ 整行**换同一句 |
| `$CASTLEGETDAYS` | `NormNpc.cs:878-890` | `HUtil32.GetDayCount(DateTime.Now, Castle.ChangeDate)`；`Castle==null ⇒ "????"` |
| R-0（城主不可达） | `GameApp.cs:213` vs `:245`；`Server.conf:365 StartPermission=0` | `MasterGuild` 恒 `null`、`Permission` 生产恒 0 ⇒ Click 的授权支与动作 switch 在 stock 部署**不可达** |

## 2. 实现

| 位置 | 改动 |
|---|---|
| `npc.rs`：`CastleVars`/`NpcVars` | 新增 `total_gold/today_income/main_door_state/under_war/war_date_text/war_list_text/get_days`；`NpcVars` 新增 `castle_official`、`wall_clock`、四个价格（缺省 = oracle `2000000/500000/300000/300000`） |
| `npc.rs`：`get_variable_text` | 拆成 `get_variable_text`（CastleOfficial 覆写层：base → `Castle==null ⇒ "????"` → 8 token）与 `base_variable_text`（原 NormNpc/Merchant 表 + 新增 `$DATETIME/$CASTLEWARDATE/$LISTOFWAR/$CASTLEGETDAYS`） |
| `npc.rs`：`take_say_abort` | `$CASTLEDOORSTATE` 无门实体 ⇒ 置一次性中止信号（复刻 NRE 从 `GotoLable` 逃出） |
| `crates/script`：`ScriptPlayer::take_say_abort` + 3 处说词点 | 缺省 `false`；引擎在 `send_say` 之前取走，为真则 `return`（整页说词不发、余下 procedure 不跑） |
| `castle.rs`：`castle_vars_for` / `castle_war_date_text` / `castle_attack_war_list_text` | `InCastleWarArea` 解析 + `CastleOfficial.cs:33-91` 所需实时快照；`GetWarDate`/`GetAttackWarList` 1:1 |
| `lib.rs`：`run_merchant_goto` | 每次脚本入口重填 `wall_clock`、`castle_vars_for` 快照、`castle_official` 标志（`NormNpc.Castle` 是对象引用 ⇒ 不能用装载期快照） |
| `lib.rs`：`CM_CLICKNPC` | type 2 走 `CastleOfficial.Click` 门：无城 ⇒ 一条红字 `SM_SYSMESSAGE`；未授权 ⇒ 零帧；授权 ⇒ `base.Click`（`@main`） |
| `lib.rs`：`CM_MERCHANTDLGSELECT` | 类型门放行 type 2（**先 base** 的 oracle 顺序） |
| `npc.rs`：`MerchantScriptBook::goto_label` | `entry.castle` 只在 `Some` 时覆写，避免把宿主的实时快照清回 `None` |

## 3. 可达性变化（本片前后）

| 路径 | 之前 | 现在（= oracle） |
|---|---|---|
| P1 `CM_CLICKNPC` type 2 | 类型门直接拒 ⇒ 零帧 | `Castle==null ⇒ 红字`；有城且未授权 ⇒ 零帧（部署即此支）；授权 ⇒ `@main` |
| P2/P3 `CM_MERCHANTDLGSELECT` 1011 | 零帧 | **`base.UserSelect` 半照跑**：`@main`/`@repairdoor`/`@aboutwar` 等说词页 + 脚本 `#ACT`（`@ask` 的 `GAMEGOLD + 5` 等） |
| `@treatdoor` | 零帧（类型门） | 零帧（**NRE 等价**：`$CASTLEDOORSTATE` 无门实体 ⇒ 整页中止）——原因与 oracle 一致 |
| 城堡动作段（open/close/repair/hire/vault/rename） | 未实现 | **仍未实现**（部署 R-0 不可达；见 §6.2） |

## 4. 合同测试（6 条）

| 用例 | 断言 |
|---|---|
| `castle_official_main_page_expands_all_tokens` | 12 token 一次展开：`游戏管理\|管理员\|1234\|56\|<墙钟>\|7\|2000000\|500000\|300000\|300000\|无效\|closed` |
| `castle_official_door_state_without_door_aborts_page` | `main_door_state=None` ⇒ `take_say_abort()` 为真且一次性 |
| `castle_official_without_castle_replaces_whole_line` | `Castle==null` ⇒ 整行 `"????"`；非 CastleOfficial 类 ⇒ token 原样 |
| `castle_official_aboutwar_whole_line_rules` | 申请表空 ⇒ 整行 `暂时没有行会攻城 .\\ \\<返回/@main>`（两 token）；战时 ⇒ `正在攻城当中...` |
| `castle_vars_for_resolves_by_war_area_not_by_index` | 皇宫图实体命中该城（gold/income/door=opened/days=5）；图外实体 ⇒ `None` |
| `t40_type2_castle_official_click_gate_and_userselect_base`（宿主级，取代 T-137 的「type 2 零帧」旧断言） | Click 无城 ⇒ **恰一条** `SM_SYSMESSAGE`「NPC不属于城堡!!!」；1011 `@main` ⇒ base 说词；`@ask` ⇒ `GAMEGOLD` 10→15；type 0 对照不变 |

## 5. 牙（3 条，各还原后 npc.rs sha256 与备份逐字节相同）

| 牙 | 变异 | 结果 |
|---|---|---|
| ① | `$CASTLEDOORSTATE` 缺门时改成 `unwrap_or("closed")`（不再中止） | **RED**：`castle_official_door_state_without_door_aborts_page` 失败 |
| ② | `Castle == null` 去掉整行 `"????"` 覆盖 | **RED**：`castle_official_without_castle_replaces_whole_line` 失败 |
| ③ | `$CASTLEWARDATE` 的整行替换退化成 token 替换 | **RED**：`castle_official_aboutwar_whole_line_rules` 失败 |

## 6. 登记偏差 / 未实现

1. **Base 变量表的其余 token 仍未实现**（`$USERCOUNT/$MACRUNTIME/$SERVERRUNTIME/$MAP/...`，既有缺口，不在本卡）。
   本卡补的 base token = 部署 `Npc_def/{沙巴克城堡管理-0150,老人-3,比奇国王-0122}.txt` 实际用到的
   **五个**：`$DATETIME` / `$CASTLEWARDATE` / `$LISTOFWAR` / `$CASTLEGETDAYS` / **`$CASTLECHANGEDATE`**
   （后者在 `老人-3` 的 `[@WarStatus]`；F3 订正：上一版报告漏列，已实现 + 用例 `.castle_changedate_formats_or_question_marks`）。
2. **城堡动作 switch 未实现**（`@openmaindoor`/`@closemaindoor`/`@repairdoornow`/`@repairwallnow1..3`/
   `@hire*now*`/`@@withdrawal/@@receipts`/`@@castlename`/`@@sendmsg`/`@exit`/`@back`）——部署 R-0
   （`MasterGuild` 恒 null、`Permission` 恒 0）使其**不可达**；`crates/world` 的纯函数
   （`castle_official_action`/`official_repair_door/wall`/N1/N5 no-op）已就绪，接线留给后续卡（T-40/T-140 剩余）。
3. **非 `@` 数据** ⇒ `SM_MENU_OK("你没有权利使用.")`（P5，构造包可达）未实现（登记）。
4. `$CASTLEGETDAYS` 在 `ChangeDate` 缺失/解析失败时展开 `"????"`（oracle 的 `DateTime` 恒有值）——登记。
5. `war_list_text` 的非空格式按 `UserCastle.cs:727-772` 实现但**无构造级用例**（部署申请表 0 字节）；
   真机窗口 W-CO-3 覆盖。
6. `Permission` 取 `sg.gm.permission`（GM 会话权限）作 `PlayObject.Permission` 的代理；生产恒 0（G12）。
7. **`$CASTLEWARDATE` 的 0 号城回退 + 写回已实现（F1）**：`Castle == null ⇒ Castle = CastleMgr.GetCastle(0)`
   （`NormNpc.cs:805-818`，**字段赋值 ⇒ 跨次对话保留**）。实现：`NpcVars.castle_zero`（宿主每次入口给
   `castles[0]` 快照）+ `castle_zero_sticky`（按 NPC `ActorId` 记账的共享集合）；`get_variable_text` 命中
   回退时**写回** `vars.castle` 并记 sticky，宿主下次解析该 NPC 的 `Castle` 时先看 sticky。无 0 号城 ⇒
   不替换（oracle `if (Castle != null)` 为假）。部署影响：`老人-3`（`@aboutwar`）/`比奇国王-0122`
   这类不在战区的 NPC 不再把字面 `<$CASTLEWARDATE>` 留在页面上。
8. **观测**：2026-10-25 03:52 部署件 `E:/MirServer/M2GameSvr/Castle/List.txt` 的 mtime 被刷新（内容仍为
   `0\r\n1\r\n`，6 字节，与部署列表一致）；疑似既有测试套件（`startup_finish` 写列表）在某个过滤器下
   写过真实路径——单跑本卡 6 条用例**不复现**（重跑前后 mtime 不变）。已登记给 Leader 排查，本卡未改任何
   `E:/MirServer` 文件内容。

## 7. R 级窗口项（`window_required: true`）

| ID | 内容 | 断言 |
|---|---|---|
| W-CO-1 | BotSrv 直发 `CM_MERCHANTDLGSELECT(1011)` 给 `沙巴克城堡管理`（同图 0150、严格 `<15` 格），`sData="@main\r"` | `SM_MERCHANTSAY(643)` 页文本逐字节（mask `$DATETIME`）：OWNERGUILD=游戏管理、LORD=管理员、CASTLEGOLD=0、TODAYINCOME=0；`Recog`=NPC ActorId |
| W-CO-2 | `@treatdoor` | **0 帧**；服务端日志恰 2 行（`[Exception] TMerchant::UserSelect... Data: @treatdoor` + 栈） |
| W-CO-3 | `@repairdoor`/`@repairwalls`/`@hirearchers`/`@commandorder`/`@day`/`@aboutwar`/`@listwar` | 页文本逐字节；价格 2000000/500000/300000；`@aboutwar`/`@listwar` 的整行替换；`@day` 的天数（墙钟，mask） |
| W-CO-4 | Click（普通玩家，有城） | 零帧零日志（R-0 未授权支） |
| W-CO-5 | 动作段不可达确认 | 注入城主/Permission 前，`@openmaindoor`/`@repairdoornow`/`@@withdrawal` 均零帧零状态变化 |

## 8. 门禁

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -D warnings` | rc=0 |
| `cargo test --workspace`（分包协议，`--test-threads=2`） | **已完成**：`-p game-svr` = 22 suites / **1528 passed** / 0 failed / 8 ignored（返工后重跑，含 F1/F3 两条新用例）；`--workspace --exclude game-svr` = 119 suites / 1692 passed / 0 failed / 54 ignored；两段 rc=0、未被 reaper 终止；合计 **141 suites / 3220 passed / 0 failed / 62 ignored** |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条） |
| 卫生四门 + `t_reservation` + `whitelist_anchor` + `m3_registry` | 见交付 status |
| 边界 | 仅 `mir2-rs/bins/game-svr/src/{npc.rs,castle.rs,lib.rs,castle/tests.rs}` + `mir2-rs/crates/script/src/engine/mod.rs` + 本报告 |

执行口径：隔离 `CARGO_TARGET_DIR=E:/tmp/t40-target` + `--offline`；全量日志 `E:/tmp/t40-ws-test.log`。

## 9. 残留

- 临时物：`E:/tmp/t40-target/`（隔离 target）、`E:/tmp/t40-ws-test.log`、`E:/tmp/t40-npc.bak`（牙备份，已还原）。
- 未 merge、未自签 review。

## 10. 返工记录（review `6c510095`，request_changes）

| 编号 | reviewer 结论 | 本片处置 |
|---|---|---|
| **F1**（阻塞，实现缺口） | `$CASTLEWARDATE` 少了 `Castle==null ⇒ GetCastle(0)` 回退 + 写回；部署 `老人-3`/`比奇国王-0122` 可达，会把字面 token 留页面上 | **已实现**：`NpcVars.castle_zero` + `castle_zero_sticky`；`get_variable_text` 命中回退即写回 `vars.castle` 并记账；宿主 `run_merchant_goto` 先看 sticky 再按 `InCastleWarArea` 解析。用例 `castle_wardate_falls_back_to_castle_zero_and_writes_back`（含「无 0 号城 ⇒ token 原样」支） |
| **F2**（Minor） | 两处旧注释仍写「type 2 门未移植」 | **已改**：`lib.rs` 的 `CM_CLICKNPC`/`CM_MERCHANTDLGSELECT` 两处注释改为实况（Click 门已接线；UserSelect 先 base 后 Castle） |
| **F3**（Minor） | 报告 §6.1「部署只用到四个」不对；`$CASTLECHANGEDATE`（`老人-3 [@WarStatus]`）未实现 | **已实现 + 订正**：`$CASTLECHANGEDATE` 按 `NormNpc.cs:856-870`（`ChangeDate.ToString("yyyy-MM-dd HH:mm:ss")`，`Castle==null ⇒ "????"`，**无** 0 号城回退）；`CastleVars.change_date_text` 由宿主格式化（日期-only 补 `00:00:00`）；用例 `castle_changedate_formats_or_question_marks` |

**全量门禁（在 `778f30a2c` 上按 tester 协议分包 + `--test-threads=2`）**：
`-p game-svr` = **22 suites / 1526 passed / 0 failed / 8 ignored**；
`--workspace --exclude game-svr` = **119 suites / 1692 passed / 0 failed / 54 ignored**；
合计 **141 suites / 3218 passed / 0 failed / 62 ignored**，两段 rc=0，**未被 reaper 终止**。
F1/F3 改动只在 `bins/game-svr`（+ docs），故返工后重跑 `-p game-svr` 段：**22 suites / 1528 passed / 0 failed / 8 ignored**（含两条新用例；其余包未受改动、沿用同一基线的绿）。
