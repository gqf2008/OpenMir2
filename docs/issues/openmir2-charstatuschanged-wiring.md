# openmir2-charstatuschanged-wiring —— 657 接线面缺口（11 处）与分批台账

> 状态：**实施中**（2026-11-20，`card/charstatuschanged-wiring`，base `00cc9e761`）。
> 来源审计：`E:\OPC\docs\audits\657-wiring-audit-20261120.md`（清点树 `c29708af7`）。
> 现成件：`pickwire::notice_frame` 的 657 臂（自通知）+ `broadcast_charstatuschanged_except_self`（观察者）。

## 0. 两条事实订正（先读）

- `SM_LEVELUP = 45`（不是 44；`crates/protocol/src/messages.rs`）。
- `drain_levelup_healthspell_with_magic` **只发 210/212 与 53**；**45/52/752 在 `drain_levelup_downlink`**。
  ⇒ "这里缺 657"的结论只能落在**真正做状态重算**的那个函数上。

## 1. census 口径

`grep -rn "RecalcAbilitys()" --include=*.cs M2Server` = **39 处，含怪物分支**
（`MonsterObject`、`ElfMonster`、`ElfWarriorMonster`、`WhiteSkeleton`、`BaseObject.cs` 的怪物重算，
以及 GM 怪物命令 `MobCommand`/`RecallMob`/`ReCallMobEx`）。
**玩家侧**才用下表（11 处缺口 + 已接 4 处：`CM_ADJUST_BONUS`、穿脱装备、`handle_eat_cmd`、施法支 A）。
**不得拿 39 当分母**。

## 2. 判据：oracle 该不该发（四列）

判断口径：oracle 只在 `RecalcAbilitys()` 体内发 657（`StatusChanged()` + `SendUpdateMsg`）。
故"Rust 有 `recalc_session_ability` 调用" ≠ "oracle 会发 657"；每条必须落到 oracle 的 `函数:行`。

| # | Rust 点（`lib.rs`） | oracle 函数:行（那里真的调 `RecalcAbilitys()`） | 结论 | 依据 |
|---|---|---|---|---|
| 1 | `apply_pending_level_up` | `ChangeLevelCommand` → `HasLevelUp` → `PlayObject.cs:5246` | **该发** | `HasLevelUp` 体内 `RecalcLevelAbilitys(); RecalcAbilitys();` |
| 2 | `apply_gm_use_item_write` | `src/Modules/GameCommand/Commands/SmakeItemCommand.cs:60` | **该发** | GM 造物写装备格后调 `PlayerActor.RecalcAbilitys()` |
| 5 | `handle_shop_cmd_inner`（升级武器） | `src/GameSrv/Npc/Merchant.cs:570` | **该发** | `user.RecalcAbilitys()` |
| 8 | `handle_spell_cmd_with_magic` 支 B | `AttPowerUp`（`PlayObject.cs:5328`） | **该发**（真缺口） | 重算了、52 发了、657 没发；Rust 注释自己引的就是 `:5328` |
| 9 | `drain_buff_downlink`（`buff_recalcs` 支） | `ActorBuffSystem.Operate:228` | **该发** | `boNeedRecalc` ⇒ `actor.RecalcAbilitys()` |
| 10 | `drain_levelup_healthspell_with_magic` | `HasLevelUp`（`PlayObject.cs:5246`） | **该发** | 见 §0 第 2 条：45/52/752 属另一函数 |
| 11/12 | `apply_pending_gm_target_ops`（`@删除技能`） | `src/Modules/GameCommand/Commands/DelSkillCommand.cs:71` | **该发** | `mIPlayerActor.RecalcAbilitys()` |
| 13 | `drain_player_need_recheck_with_magic` | `PlayObject.Message.cs:432`（另 `Operate.cs:455/544`） | **该发** | 穿脱别名路径内调 `RecalcAbilitys` |
| 14 | `drain_player_hunger` | `RefMyStatus`（`PlayObject.Base.cs:290`） | **该发** | `RefMyStatus` 体内先 `RecalcAbilitys()` |
| 15 | `drain_player_revival_with_magic` | `ItemDamageRevivalRing`（`PlayObject.cs:5378`） | **该发** | 复活戒指打碎后调 `RecalcAbilitys()` |

> 实施中若某条反证不成立：**划掉并留痕**（写明反证），不得静默删除。

## 3. `#8` 单独一批（同时删错误注释）

`#8`（施法支 B）自称"657 末参"并写了 `e.vitals.hit_speed`，但本支无 657 出帧。
批次 C 里同时**删掉那句错误注释**，避免下一个人把它当"已接"。

## 4. `#9` 用例名判读（写进卡，防"已覆盖"误判）

`drain_buff_downlink_emits_charstatuschanged_and_healthspellchanged` 断言的是
**`rec.buff_status_changes`** 支（该支确实发 657）；缺 657 的是 **`rec.buff_recalcs`（`boNeedRecalc`）** 支。
**名字不算撒谎，但只覆盖一半** ⇒ 按 §25 补断言或拆两条用例，名字各自只说自己那条支。

## 5. `status` 来源纪律

逐点核"oracle 那条路径当时读的是哪个值"（`PlayObject.cs:2873` 的 `CharStatus = GetCharStatus()` 在
`StatusChanged()` 之前）。Rust 侧取 `world.get_char_status(actor_id)`（与既有 4 处同源）。
**禁止统一塞常量**；某处找不到来源先报，不猜。

## 6. 分批

- **批 A**：#1 `apply_pending_level_up`、#2 `apply_gm_use_item_write`、#5 `handle_shop_cmd_inner`、
  #10 `drain_levelup_healthspell_with_magic`
- **批 B**：#9 `drain_buff_downlink`、#13 `drain_player_need_recheck_with_magic`、
  #14 `drain_player_hunger`、#15 `drain_player_revival_with_magic`
- **批 C**：#11/#12 `apply_pending_gm_target_ops` + #8（含删错误注释）

每处：自（`sm_charstatuschanged(actor, status, hit_speed)` 或 `notice_frame`）+ 广
（`broadcast_charstatuschanged_except_self`）。每批：单独提交、三 fixture 不重复用例（只自/只广/都开）、
全量复跑（§38：冲突解决后必跑）。

## 7. 台账

| 批 | 处数 | 提交 | 门禁三数 | 备注 |
|---|---|---|---|---|
| A(1/4) | #1 `apply_pending_level_up` | `483d8d33e` | 2093 / 0 / 65 | 自+广；用例四断言；牙 w1–w3 |
| A(2/4) | #2 `apply_gm_use_item_write` | `6c0807935` | 2094 / 0 / 65（guild 126/0） | 自+广在函数内发；签名 +online/gates；帧内序 **T-288** |
| A(3/4)(4/4) | #5 / #10 | **未接线** | 2094 / 0 / 65 | 657 直发会按调用序插错（155 应排最后）⇒ 登记 **T-289 / T-290**；统一解见 `openmir2-tick-frame-priority-queue` |
| B | #9 #13 #14 #15 | **未接线** | — | 同上，已登记 **T-292 / T-293 / T-294**（#11/#12 = **T-291**） |
| C | #11/#12 + #8 | 未开始 | — | #8 含删错误注释 |
| B | 4 | （待填） | | |
| C | 2+#8 | （待填） | | |

## 8. 已知偏差

> **口径（2026-11-20，`opc-19`）**：657 在 oracle 走队列（155），本仓直发则序不可控 ⇒
> **一接就错**。因此 #5/#10/#11–#15 **保持未接线**，各自登记偏差（T-289–T-294），
> 统一解由 `openmir2-tick-frame-priority-queue` 一张卡承接；不再各自发明"按阶段排"的近似
> （反例：把 657 延后到 tick 末端会排到 `RM_SYSMESSAGE`(252) 之后，换个方向错）。

### T-288：GM 改属性路径的帧内序（657 先于 203）

- **事实**：oracle 线上序是 `SM_UPDATEITEM(203)`（`SendDefMessage` **直发**）先、
  `RM_CHARSTATUSCHANGED → 657`（**入队**）后；本宿主在 `apply_gm_use_item_write` 的
  `recalc_session_ability` 之后**直发** 657 ⇒ 657 先、203 后（差一帧）。
- **根因（一句话）**：203 是直发、657 走队列，直发先到；要完全对齐只能把 657 也改成入队——
  那属 **pump 模型改造（§29 域）**，不在本批。
- **可及性**：本路径仅 GM 可达（`SmakeItemCommand` 需 `Permission`，现网 `PlayObject.Permission` 恒 0）
  ⇒ 生产不可达；客户端对 657（状态位）与 203（物品栏刷新）无顺序依赖。
- **留痕**：白名单 `mir2-rs/tests/parity/whitelist.md` 的 **T-288**；用例
  `gm_smake_frame_order_is_updateitem_then_sysmsg` 的期望按本偏差更新，断言消息点名 T-288。
- **纪律**：帧序对齐优先于"测试好看"；接受登记、**不接受静默**。
