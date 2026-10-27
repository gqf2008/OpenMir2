# 城堡动作 switch（CastleOfficial 动作段）构造级（tier R）

- 卡：`openmir2-castle-official-actions`（owner `openmir2-dev-2`，branch `card/castle-official-actions`，base `02f83a1a2`）。
- 写集：`mir2-rs/bins/game-svr/src/{castle.rs,castle/tests.rs,lib.rs}` + 本报告。
- **可达性**：oracle 的动作段在 `Castle.IsMasterGuild(MyGuild) && PlayObject.IsGuildMaster()` 之后
  （`CastleOfficial.cs:109-110`），部署 R-0（`MasterGuild` 恒 null、`Permission` 恒 0、唯一 type 2 在 0150 战区）
  ⇒ **真机不可达**。本片为**构造级**实现 + 判据；窗口登记 `window_required`。

## 1. oracle 锚（逐标签）

| 标签 | oracle | 语义 |
|---|---|---|
| `@openmaindoor` / `@closemaindoor` | `CastleOfficial.cs:199-206` → `UserCastle.cs:890-909` → `CastleDoor.cs:44-76` | `MainDoorControl(false/true)`：幂等开关；`RM_DIGUP`/`RM_DIGDOWN` + 15/12 条 `SetMapXyFlag`。**注意：这两支不调 `GotoLable(@main)`**（`@main` 页来自脚本 `Npc_def:35-44` 自带的 `goto @main`，本仓由 base 半的 `Goto` 动作产生）⇒ 实现 `say_main=false` |
| `@repairdoornow` | `:207-211` → `RepairDoor :358-381` | `TotalGold < RepairDoorPrice` ⇒ 红「城内资金不足!!!」；否则 `RepairDoor`（`UnderWar`/满血/60s 严格 `>` 门）⇒ 成功扣款 + 绿「修理成功。」、不成功绿「城门不需要修理!!!」 |
| `@repairwallnow1..3` | `:212-226` → `RepairWallNow :383-406` | 同构（价格用 `RepairWallPrice`；墙索引 1/2/3） |
| `@hireguardnow*` / `@hirearchernow*` | `:227-238` | **N5（T-40）**：`Substring` 长度实参恒越界 ⇒ 被吞 ⇒ 零帧零状态零提示 |
| `@@withdrawal` / `@@receipts` | `:155-198` | **N1（T-38）**：接口重载抛 `NotImplementedException` ⇒ 被 Castle catch 吞 ⇒ 无可观测效果 |
| `@@castlename` | `:139-154` | 改名 + `SaveConfigFile` + 成员名牌刷新；**本仓无 SabukW 写侧** ⇒ 登记未实现（R-0 亦不可达） |
| `@@sendmsg` | `:134-138` → `SendCustemMsg :408-420` | `!SubkMasterSendMsg` ⇒ 红字提示；否则 `BoSendMsgFlag` 清标志 + 全服广播（`MsgType.Castle` ⇒ 红字） |
| `@@offlinemsg` | `:127-133` | `IsOffLineMsg` 恒 false（`Npc_def` 头被 `boFlag=false` 跳过）⇒ 不可达 |
| `@exit` / `@back` | `:239-250` | `SM_MERCHANTDLGCLOSE` / 回 `ScriptGoBackLable ?: @main`（base 已发一份 ⇒ 双帧/双说词） |

## 2. 实现

`bins/game-svr/src/castle.rs::castle_official_action_exec`（构造级入口）：
- 标签判类复用 `mir2_world::castle::castle_official_action`（精确匹配优先、`CompareLStr` 前缀次之）。
- **open/close**：`main_door_control(false/true)` → 摊平 `CastleEvent::Structure` → `SetMapXyFlag` 落地图 +
  `SM_DIGUP`/`SM_DIGDOWN` 按 `SendRefMsg` 的 ±`SendRefMsgRange` 格扫出帧；**不回 `@main`**（F1：oracle 只调
  `MainDoorControl`，`@main` 由脚本 `goto` 产生，再回一页会重复）。
- **repair**：`official_repair_door/wall(price, now_tick)` → 结果文案（成功/不需要 = 绿，资金不足 = 红）+ 事件帧；回 `@main`。
- **N2 统一（merge_result `29e240b9` Minor，follow-up）**：emit 的 `SM_ALIVE` `CharDesc.Status`
  与 `drain_monster_ref_msgs` 的 RM_ALIVE 支**同一来源**（展示层 `d.status`，无登记 ⇒ 0）；
  `RM_DIGUP` 保持实体 `char_status`（= drain 侧 `m.status` 的发帧时刻语义）。现网门/墙无 buff ⇒ 两源恒等。
- **hire**：`HireUnreachable` ⇒ `log=Some("N5")`，零帧零状态（照搬）。
- 未命中标签 ⇒ `None`（oracle switch 无该分支）。
- **N1 统一（reviewer-20）**：`handle_click_npc_cmd` 的 `Castle` 解析改为 **sticky 优先**——
  一旦 `$CASTLEWARDATE` 把 `Castle` 写回 0 号城，它就不再随位置变化：
  `if sticky_zero { (!castles.is_empty()).then_some(0) } else { castle_war_area_at(...) }`。

## 3. 判据（构造级，`castle/tests.rs`）

| 用例 | 断言 |
|---|---|
| `castle_official_action_open_close_emits_door_frames` | open ⇒ `SM_DIGUP` 到 ±12 格玩家且 `main_door.opened=true`；close ⇒ `SM_DIGDOWN` 且 `opened=false`；**两者 `say_main=false`（不回 @main）**；并断言 `Series=MakeWord(Dir,Light)` 与尾 `Param1=GetFeature`（展示层非零，F1） |
| `castle_official_action_repair_gold_and_messages` | 资金不足 ⇒ 红「城内资金不足!!!」且不扣款；钱够+满血 ⇒ 绿「城门不需要修理!!!」且不扣款 |
| `castle_official_action_repair_wall_emits_wall_frames_after_60s`（返工新增） | 受损左墙（`(4,5)`，**与门是两个不同 actor**）+ `StruckTick=0` ⇒ `修理成功。` 绿、扣 `RepairWallPrice`、**帧的 Recog = 墙的 ActorId**（F2：只查主门会整帧丢失） |
| `castle_official_action_hire_is_n5_noop` | `@hirearchernow1/12`、`@hireguardnow3` ⇒ 零帧/零提示/不回 @main/`log=N5`；`@main` ⇒ `None` |
| `castle_official_action_alive_status_uses_display_layer`（N2 follow-up 新增） | emit 的 `SM_ALIVE`（墙修复帧）`CharDesc.Status` 取**展示层** `d.status`（与 `drain_monster_ref_msgs` 的 RM_ALIVE 支同源）；实体 `char_status` 故意给 `0x1234` 使两源可分 ⇒ 登记态读 `0x77`、未登记态读 `0` |

**牙（各还原后 `castle.rs` sha256 与备份逐字节相同）**：
① `let close = matches!(..., CloseMainDoor)` 改成 `OpenMainDoor` ⇒ open/close 用例 RED；
② open/close 分支的 `say_main` 改回 `true` ⇒ open/close 用例 RED（F1 的钉）；
③ `castle_unit_entity_at` 收回「只查 `main_door`」⇒ 墙修复用例 RED（F2 的钉）；
④ emit 的 `light`/`feature` 改回硬编码 0 ⇒ open/close 用例 RED（返工 2 F1 的钉；reviewer 实测两颗各红一次）；
⑤ RM_ALIVE 支的 status 改回实体 `char_status` ⇒ N2 用例两格都 RED（N2 的钉）。

## 4. 登记（未实现 / 不可达）

1. `@@castlename`（改名 + `SaveConfigFile`）与 `@@sendmsg` 的广播支**未接线**：前者缺 `SabukW` 写侧、后者需
   `BoSendMsgFlag` 会话位；两者在部署与构造级都需先注入城主（R-0）⇒ 随后续片。
2. `@exit`/`@back` 的双帧/双说词由 base（Merchant 半）负责，本片不重复产出。
3. 动作段**未接进 1011 漏斗**：门是 `IsMasterGuild && IsGuildMaster`，宿主侧 `master_guild` 恒 `None`（R-0）
   ⇒ 接线只会是死代码；本片以**可判的构造级入口**交付，接线留给「城主物化」片。
4. 窗口项 `W-CO-6`：注入城主/`CanJmpScriptLableMap` 后复拍 open/close/repair 的帧与 60s 门（**依赖 owner
   批准的测试专用注入**，非本次窗口）。

## 5. 门禁

- 定向：`castle_official_action_*` 3/3 + `castle_official` 9/9 全绿；fmt / clippy（workspace all-targets `-D warnings`）0；
  `git diff --check 02f83a1a2 <head>` 见 patch body；四卫生门 + t_reservation + whitelist_anchor 见 status。
