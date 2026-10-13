# M3④-实现 D 分片 D②：升级链（`RecalcLevelAbilitys`+`RecalcAbilitys` + `SM_LEVELUP(45)`/`SM_ABILITY(52)`/`SM_SUBABILITY(752)`/`SM_HEALTHSPELLCHANGED(53)`）+ F-1/F-2 返工

> 卡：`openmir2-m3-drop-exp-wiring`（issue `58825bcc`，承接 `openmir2-m3-drop-exp-align` 十项之一）。
> 本片是 D 家族在 **D①（击杀经验链）** 之后的**升级后果**这一段；**掉落落地 / `SM_ITEMSHOW(610)`** 仍属未做
> （A/B 裁决仍归 owner，见 §5；本片**不擅改**）。
> 本轮 = reviewer-6 `c5a90bf4` 的 `request_changes`（F-1 帧序、F-2 `BodyLuck`）+ owner 裁决 `b2851a44` 的返工。
> base = `origin/master` **`a443296e`**（交付前 `git merge origin/master`，merge 提交 `bdc53848`）；head = 分支 `drop-exp-host`。
> 工作目录 `<worktree>/mir2-rs`；隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-f2-target`。

## 0. 一句话

D① 只把经验发出去（`SM_WINEXP(44)` + 会话同步），但 oracle `GetExp`（`PlayObject.Base.cs:149-166`）在
`Abil.Exp >= Abil.MaxExp` 时还会做：`AddBodyLuck(dwExp*0.002)`、`HasLevelUp`（重算等级派生能力 +
`SM_LEVELUP`/`SM_ABILITY`/`SM_SUBABILITY`）、`AddBodyLuck(100)`、`IncHealthSpell(2000,2000)`（回满血蓝 →
`SM_HEALTHSPELLCHANGED`）。本片按 oracle 补齐**整段后果 + 帧序 + 落账**。

## 1. oracle 锚点（原文见 `01-oracle-anchors.txt`）

| 步骤 | 行号 |
| --- | --- |
| `GetExp`（`Abil.Exp +=`、`AddBodyLuck(dwExp*0.002)`、`RM_WINEXP`、升级块） | `PlayObject.Base.cs:149-166`：`:152` / `:154` / `:157-159` / `:161` / `:162` / `:164` |
| `AddBodyLuck`（本体 / 读取点 / 落库） | `PlayObject.cs:4304-4323`；`Merchant.cs:677/698/719`；`ConditionProcessingSys.cs:163`；`WorldServer.cs:1158/1290` |
| `MAXUPLEVEL = byte.MaxValue` / `BODYLUCKUNIT = 5000` | `MessageSettings.cs:7` / `:9` |
| `HasLevelUp`（`MaxExp` / `RecalcLevelAbilitys` / `RecalcAbilitys` / `RM_LEVELUP`） | `PlayObject.cs:5242-5248` |
| `RM_LEVELUP` 处理器（45 → 52 → 752） | `PlayObject.Message.cs:1670-1676` |
| `IncHealthSpell` / `HealthSpellChanged` / 53 处理器 | `BaseObject.cs:952-975` / `:700-710` / `PlayObject.Message.cs:1736-1739` |
| **帧序 = `MsgQueue` 优先级字节出队序** | `BaseObject.Message.cs:26-95`（入队 `(byte)wIdent`）、`:275-304`（`SendUpdateMsg` 同 ident 去重）、`BaseObject.Base.cs:26-30`（升序出队） |
| 消息号 | `Messages.cs:206/211/576/678/689` |

**F-1 关键口径（入队序 ≠ 线上序）**：`GetExp` 把 `RM_WINEXP(8075→139)`、`RM_LEVELUP(8086→150)`、
`RM_HEALTHSPELLCHANGED(10052→68)` 入**同一个 `MsgQueue`**，`BaseObject.Run()` 按优先级字节**升序出队**
⇒ 同一 socket 上真实线上序 = **53(68) → 44(139) → 45(150) → 52 → 752**；源码调用序 `44→45→53` 不是线上序。
（详见 `01-oracle-anchors.txt` [8]。）

## 2. 本片实现（写集 `crates/world/**` + `bins/game-svr/**`）

- `crates/world/src/combat.rs`：`award_kill_exp` 的 `leveled` = **进了升级块**（oracle `if Abil.Exp >= Abil.MaxExp`），
  `Abil.Level++` 用 `saturating_add(1)` 复刻 `< MAXUPLEVEL` 门；满级仍置位（宿主仍要重算/回血/出帧）。
- `crates/world/src/world.rs`：`TickRecord::exp_awards` 文档同步（第 3 位语义）。
- `bins/game-svr/src/smframe.rs`：`sm_levelup(exp, level)`（`SM_LEVELUP(45)`，无尾）。
- `bins/game-svr/src/main.rs`：
  - `recalc_session_ability(sg, catalog)`（装备/卸下与升级共用的**单一实现**）；
  - **F-1**：升级链按 oracle 出队序拆两段 + 一个定序入口：
    * `drain_levelup_healthspell`（**必须先于 44**）：同步 `exp/level` → `recalc_session_ability` →
      `IncHealthSpell(2000,2000)` → 同步世界实体 → 53；
    * `drain_combat_downlink`：受击/死亡 + 44；
    * `drain_levelup_downlink`：45 → 52 → 752；
    * `drain_kill_exp_chain`：把三段按 **53 → 44 → 45/52/752** 固定在一处，`sim_loop` 与用例共用；
    * **同 tick 去重**：本 tick 进升级块的对象，恢复段的 53 被升级段的 53 取代（`SendUpdateMsg` 去重），
      `sim_loop` 算 `leveled_this_tick` 集传给 `drain_recovery`（状态仍消费、不回帧）；
  - **F-2**：`add_body_luck`（`PlayObject.cs:4304-4323`，含 `round_ties_even` 银行家舍入 + clamp）、
    `body_luck_level_of`（`Initialize()` 的 `AddBodyLuck(0)`）；`EquipInputs` 新增 `body_luck`/`body_luck_level`，
    进世界从记录装载；`drain_combat_downlink` 经验循环按 oracle 次序落账（`dwExp*0.002` 先、升级 `+100` 后）；
    `build_save_record` 写 `characters.BodyLuck`。

## 3. 判据（构造级，见 `03-tests.txt`）

| 用例 | 位置 | 钉住 |
| --- | --- | --- |
| `levelup_downlink_sends_levelup_frames_heals_and_syncs_entity` | `bins/game-svr/src/main.rs` | 帧序 **53→44→45→52→752**（经 `drain_kill_exp_chain`）、`SM_LEVELUP` 头、`MaxHP` 等级派生、回血满额、实体同步、`body_luck==101.0` |
| `add_body_luck_matches_oracle_vectors` | 同上 | 10 组向量：银行家舍入、`±5*unit` 累加门、`[-10,5]` clamp |
| `body_luck_accumulates_on_getexp_and_persists` | 同上 | 非升级击杀 `body_luck=5000`/level=1，且 `build_save_record` 带出 |
| `recovery_healthspell_is_superseded_by_levelup_in_same_tick` | 同上 | 升级块 tick 的恢复段 53 被取代（不回帧，状态仍消费）；不在集合则照回 |
| `kill_exp_marks_levelup_block_even_at_max_level` | `crates/world/src/world.rs` | `leveled` = 进升级块；满级 255 仍置位 |

## 4. 改坏必红（8 条，原始件 `40..48`；还原核对 `43`）

| 编号 | 变异 | 必红用例 | rc |
| --- | --- | --- | --- |
| RC-D2-A | `sm_levelup` 头 recog/param 交换 | `levelup_downlink_...` | 101 |
| RC-D2-B | 删 `IncHealthSpell(2000,2000)` | 同上 | 101 |
| RC-D2-C | 删 `recalc_session_ability` | 同上 | 101 |
| **RC-D2-D** | **交换 `drain_kill_exp_chain` 前两行（53 落到 44 后）** | 同上 | 101 |
| **RC-D2-E** | **`round_ties_even()` → `round()`** | `add_body_luck_...` | 101 |
| **RC-D2-F** | **删 `rec.data.body_luck = sg.equip.body_luck`** | `body_luck_accumulates_...` | 101 |
| **RC-D2-G** | **删升级块 `AddBodyLuck(100)`** | `levelup_downlink_...` | 101 |
| **RC-D2-H** | **删 `drain_recovery` 的去重分支** | `recovery_healthspell_is_superseded_...` | 101 |

## 5. 未做 / 登记（交 owner）

- **掉落落地 + `SM_ITEMSHOW(610)`（A/B 裁决）**：oracle `MapItem.ItemId` 无赋值点（`MapItem.cs:41` 注释）+
  `Envirnoment.AddItemToMap` 首行 `ItemId==0 ⇒ false` ⇒ oracle 死亡掉落**恒不落地**；Rust `settle_deaths`
  会落地 ⇒ 仍等 owner §6.0-3 裁决（见 `drop-exp-drop-side-20261013/README.md` §4）。本片**不擅改**。
- **`WinExp` 的 `LimitExpLevel` 支**：默认 `LimitExpLevel=1000 > 255`（`GameSvrConf.cs:1838`）⇒ 生产不可达（登记）。
- **`ShowHp` 的 53 广播**：`ShowHp` 由技能「心灵启示」置位，技能层未移植 ⇒ 生产恒 false（与 `drain_recovery`/`drain_stamina` 同口径）。
- **`GainExp` 组队分支**（`PlayObject.Base.cs:764-817`）：组队系统（M3⑦）未落地，无输入，登记。
- **`LastHiter`/`ExpHiter` 墙钟 30s/6s 过期**（D① 已登记）：仍属后续。
- **whitelist T-32**（`mir2-rs/tests/parity/whitelist.md:174`）里「无 `AddBodyLuck`、无升级 `IncHealthSpell`」已过时
  （D①/D② 已实现）⇒ **请 owner 在卡收口时收窄/退役该条目**（白名单文案 owner 收口，本片不动它）。
- **真机 100 杀**（卡 DoD②③⑤）：要独占窗口 ⇒ 等 owner 发放（起栈禁接管道）。

## 6. 门禁 / 证据

- 四门禁（隔离 `E:/tmp/m2svr-f2-target`，`--offline`）：fmt rc=0 / clippy `-D warnings` rc=0 / msgcodegen rc=0（604）/
  workspace **91 suites / 820 passed / 0 failed / 18 ignored**；同树去本片改动基线 **91/817/0/18** ⇒ **+3**。
- 证据 hash：`verify_sha256sums.py --check-extra` + `eolcheck`。
- 边界：`git diff origin/master..HEAD -- src sql MirClient` 空（oracle 冻结）。
- 未起栈 / 未占端口 / 未连 MySQL（本片判据全在构造层）。

## 7. reviewer-6 F-1/F-2 逐条回复

- **F-1（Important，帧序）**：**采纳 (a)**。确认 oracle 出队序成立（`01-oracle-anchors.txt` [8] 逐行 + 常量；
  reviewer-6 的 `PriorityQueue` 实拍亦在卡上）。实现：升级链拆 `drain_levelup_healthspell`（53，先于 44）+
  `drain_levelup_downlink`（45/52/752），由 `drain_kill_exp_chain` 定序；用例 ident 断言改为
  `[53,44,45,52,752]`；新增 RC-D2-D（交换定序 ⇒ 必红）。代码注释/证据里「入队序 = oracle 帧序」的表述已订正。
  同 tick 内恢复/体力点段的 53 已按 `SendUpdateMsg` 去重建模：`sim_loop` 把`leveled_this_tick` 集传给 `drain_recovery`，
  命中者本段不回帧（状态仍消费），由 `drain_levelup_healthspell` 出唯一一条（新用例 + RC-D2-H）。
  登记：由最低优先级（255）延时消息驱动的升级不在本模型内。
- **F-2（Important，BodyLuck）**：**采纳 (a)**，撤销「死字段」结论。实现见 §2；读取点/落库行号见
  `01-oracle-anchors.txt` [7](a)/[9]。新增 RC-D2-E/F/G 三条红检。`characters.BodyLuck` 现随会话实时落库。
