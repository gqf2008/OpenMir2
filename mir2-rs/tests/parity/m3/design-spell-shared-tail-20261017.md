# `MagicManager.DoSpell` 共享尾（`:644-670`）接线设计件 —— case 对照 / default 支 / 缺口 / 分期

- 卡片：`openmir2-spell-shared-tail-wiring`（设计件，tier=L；实现片按本件再判 R/L）
- 基线：`6eea609d0`（生产代码与 `18c7ffe56` 相同；`18c7ffe56` = `feat(game-svr): 接线 PlayObject.DoSpell 包装层（扣 MP + 53 帧先于 RM_SPELL + 蓝不足 639）（tier=R）`）
- 范围：**只**讨论 `MagicManager.DoSpell` 的共享尾（`:644-670`）与它的前置门（`:151` / `:155` / `:159` / `:160-163` / `:167`）；
  效果体本身只在「缺口/分期」里按已建模块点名，不重述其算法。
- 引用纪律：
  - oracle（`src/**`，冻结）一律 `文件:行号`，本件全部逐条 `awk NR` 实测；
  - Rust（`bins/game-svr/src/**`、`crates/world/src/**` 会随并行车道位移）**只写符号 / 块名 / 函数名，不写行号**。
- 只读声明：本件在自建 worktree `/e/tmp/impl-sharedtail2`（`git worktree add … 6eea609d0 --detach`）产出；
  共享主检出未改一行；未改 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`、`E:/MirServer/**`；
  未起进程、未占端口、未写库、未删文件。

---

## 1. oracle 结构基线（逐行实测）

`MagicManager.DoSpell(PlayObject, UserMagic, short nTargetX, short nTargetY, IActor targetObject)`
（`src/M2Server/Magic/MagicManager.cs:145`）：

```text
:147-154   if (IsWarrSkill(userMagic.MagIdx)) return false;            ← 战士门（9 个魔号，见 §3.2）
:155-158   if (|CurrX-nTargetX| > MagicAttackRage || |CurrY-nTargetY| > MagicAttackRage) return false;
:159       SendRefMsg(RM_SPELL, Magic.Effect, nTargetX, nTargetY, Magic.MagicId, "");   ← 无条件、恰一条
:160-163   if (targetObject != null && targetObject.Death) targetObject = null;        ← 死亡归一
:164       bool boTrain = false;
:165       bool boSpellFail = false;
:166       bool boSpellFire = true;
:167-170   if ((SoftVersionDateEx == 0 && ClientTick == 0) || Magic.MagicId > 80) return false;  ← 版本/魔号门
:172-643   switch (Magic.MagicId) { …45 个 case 标签… }                               ← **无 default:**
:644-647   if (boSpellFail) return false;                                              ← ①
:648-658   if (boSpellFire) SendRefMsg(RM_MAGICFIRE, 0, MakeWord(EffectType, Effect),  ← ②
                                        MakeLong(nTargetX, nTargetY), targetObject==null ? 0 : targetObject.ActorId, "");
:659-669   if (Level < 3 && boTrain && TrainLevel[Level] <= Abil.Level) {               ← ③
             TrainSkill(userMagic, Random(3) + 1);                                      ←   唯一取数
             if (!CheckMagicLevelUp(userMagic)) SendSelfDelayMsg(RM_MAGIC_LVEXP, …, 1000);
           }
:670       return true;
```

调用方（玩家路径）`PlayObject.Attack.cs`：

- `:1118` `switch (UserMagic.MagIdx)`；8 条独立 `case`（`:1120/:1136/:1152/:1168/:1187/:1218/:1234/:1253` =
  12/25/56/26/27/34/38/43）**不进 `default`**，因此**根本不调用** `DoSpell`；
- `:1269` `default:` → `:1270` `Dir = GetNextDirection(...)` → `:1272-1277` `CretInNearXy` 命中则**就地**改写
  `nTargetX/nTargetY` → `:1278` `DoSpell(UserMagic, nTargetX, nTargetY, spellObject)`；
- `:1278-1281` `DoSpell` 返回 false ⇒ `SendRefMsg(RM_MAGICFIREFAIL, 0,0,0,0,"")`（客户端 `SM_MAGICFIRE_FAIL(639)`）。

包装层（`PlayObject.Base.cs:1505-1531`，`18c7ffe56` 已接）：

```text
:1510   if (!IsWarrSkill(MagIdx)) {
:1512     nSpellPoint = GetSpellPoint(UserMagic);        // :1369-1372
:1515-1518  if (WAbil.MP < nSpellPoint) return false;    // ⇒ 调用方 639；未发 RM_SPELL、零取数
:1519-1520  DamageSpell(nSpellPoint); HealthSpellChanged();   // MP 先扣、53 帧先发
:1522     result = MagicManager.DoSpell(...);            // 内含 :155 距离门
:1525-1529  catch (Exception) { … }                      // **吞异常**，result 保持 false
```

⇒ 三条与时序有关的硬结论：

1. **MP 扣减 + 53 帧早于 `:159`**，也早于 `:155` 距离门（距离门在 `MagicManager.DoSpell` 内）⇒ 越界施法
   **会扣蓝**、会发 53，但不发 `RM_SPELL`，并由调用方发 639。
2. **共享尾在 switch 之后**，因此 `RM_MAGICFIRE` 的坐标/目标读的是**被 case 就地改写后**的局部变量
   （见 §2.2），不是 `:159` 那帧的入参。
3. `PlayObject.Base.cs` 的 `catch` 会把 case 体内异常变成「`DoSpell` 返回 false ⇒ 639」，且**尾整段不执行**。

---

## 2. case 对照总表（`:172-643`，45 个 `case` 标签 / 38 个 case 块）

列说明：
- 「boSpellFire」= 该 case 是否改变共享尾的发帧门；`—` = 保持不变（`true`）。
- 「boTrain」= 置 `true` 的条件（贴 oracle 行号）；`—` = 从不置真。
- 「case 内自有帧」只列**可直接归到该 case/其 helper** 的 `Send*`；`(helper)` 标注帧发生在被调函数内。
- 「Rust 现状」只写符号/块名。「已接线」= `handle_spell_cmd` 有生产支；「尾已接/未接」专指共享尾。

| # | 常量(值) | oracle 行区间 | boSpellFire | boTrain 赋值（行号 + 条件） | case 内自有帧 | Rust 现状（符号/块名） |
|---|---|---|---|---|---|---|
| 1 | `SKILL_FIREBALL`(1) | `MagicManager.cs:174-203` | — | `:186` 命中门内 `targetObject.Race >= Animal` | `:183` `RM_DELAYMAGIC`（self-delay） | `magic::power_get_power_plus_mc` + `magic_effect::mpow/get_power`（离线向量）；未接线 |
| 2 | `SKILL_FIREBALL2`(5) | `:175`, `:174-203`（同块） | — | 同 #1（`:186`） | 同 #1（`:183`） | 同 #1 |
| 3 | `SKILL_HEALLING`(2) | `:204-224` | — | `:217` `targetObject.WAbil.HP < MaxHP` | `:216` `RM_MAGHEALING`（目标）；`:221` `RM_ABILSEEHEALGAUGE`（`SendMsg(targetObject, …)`） | `magic::power_heal_get_power_plus_sc2`（离线）；未接线 |
| 4 | `SKILL_AMYOUNSUL`(6) | `:225-258` | — | `:250` 在 `Random(AntiPoison+7)<=6` 内且 `Race==Play或>=Animal` | `:241/:245` `RM_POISON`（绿毒/红毒，目标） | `magic::power_poison`（离线）；未接线 |
| 5 | `SKILL_FIREWIND`(8) | `:259-264` | — | `:262` `MagPushArround(...) > 0` | 无（`CharPushed` 内 `RM_PUSH`） | **已接线**：`World::cast_firewind_spell` + 尾 `magic::do_spell_tail` |
| 6 | `SKILL_FIRE`(9) | `:265-276` | — | `:273` `MagPassThroughMagic(...) > 0` | 无直接帧（helper 内部逐格帧） | `magic::power_get_power_plus_mc`（离线）；未接线 |
| 7 | `SKILL_SHOOTLIGHTEN`(10) | `:277-288` | — | `:285` `MagPassThroughMagic(...) > 0` | 无直接帧（helper 内部逐格帧） | `magic::power_get_power_plus_mc`（离线）；未接线 |
| 8 | `SKILL_LIGHTENING`(11) | `:289-314` | — | `:302` 在 `Random(10)>=AntiMagic` 内且 `Race>=Animal` | `:299` `RM_DELAYMAGIC`（self-delay） | `magic::power_lightening`（离线）；未接线 |
| 9 | `SKILL_FIRECHARM`(13) | `:315`（外 `:315-393`），内 `:328-351` | — | `:341` `Race>=Animal`（`:338` 延迟帧之后） | `:338` `RM_DELAYMAGIC`；`:349` 未命中 ⇒ `targetObject = null` | `magic::power_get_power_plus_sc`（离线）；未接线 |
| 10 | `SKILL_HANGMAJINBUB`(14) | `:316`（外 `:315-393`），内 `:352-358` | — | `:356` `MagMakeDefenceArea(...) > 0` | 无 | **已接线（效果体）**：`World::cast_defence_area_spell`；**尾未接**；护身符门未接 |
| 11 | `SKILL_DEJIWONHO`(15) | `:317`（外 `:315-393`），内 `:359-365` | — | `:363` `MagMakeDefenceArea(...) > 0` | 无 | 同 #10 |
| 12 | `SKILL_HOLYSHIELD`(16) | `:318`（外 `:315-393`），内 `:366-371` | — | `:369` `MagMakeHolyCurtain(...) > 0` | helper `:1168` `RM_MAKEHOLYSEIZEMODE`（目标） | `magic::power_holy_shield`（离线）；未接线 |
| 13 | `SKILL_SKELLETON`(17) | `:319`（外 `:315-393`），内 `:372-377` | — | `:375` `MagMakeSlave(...)` | helper：召唤落场（无直接 `Send*`） | **已接线**：`slave::cast_slave_spell`；尾已接 |
| 14 | `SKILL_CLOAK`(18) | `:320`（外 `:315-393`），内 `:378-383` | — | `:381` `MagMakePrivateTransparent(...)` | 无 | `magic::power_transparent`（离线）；未接线 |
| 15 | `SKILL_BIGCLOAK`(19) | `:321`（外 `:315-393`），内 `:384-389` | — | `:387` `MagMakeGroupTransparent(...)` | helper `:1221` `RM_TRANSPARENT`（目标） | `magic::power_transparent`（离线）；未接线 |
| 16 | `SKILL_TAMMING`(20) | `:394-402` | — | `:399` `MagTamming(...)` | helper `:712/:812` `RM_MAKEHOLYSEIZEMODE` | **已接线**：`slave::cast_tamming_spell`；尾已接 |
| 17 | `SKILL_SPACEMOVE`(21) | `:403-411` | **`:406` 置 `false`**（先于它的是 `:405` `RM_MAGICFIRE`） | `:409` `MagSaceMove(...)` | `:405` `RM_MAGICFIRE`（actorId=`targerActors`）；helper `:881` `RM_SPACEMOVE_FIRE2` | **已接线**：`World::cast_space_move_spell_with`；尾已接且 `bo_spell_fire=false` |
| 18 | `SKILL_EARTHFIRE`(22) | `:412-417` | — | `:415` `MagMakeFireCross(...)` | 无（只落火墙格） | `magic::power_fire_wall`（离线）；未接线 |
| 19 | `SKILL_FIREBOOM`(23) | `:418-423` | — | `:421` `MagBigExplosion(...)` | helper `:1111` `RM_MAGSTRUCK`（目标） | 仅威力 helper（离线）；未接线 |
| 20 | `SKILL_LIGHTFLOWER`(24) | `:424-430` | — | `:428` `MagElecBlizzard(...)` | helper `:1138` `RM_MAGSTRUCK`（目标） | 仅威力 helper（离线）；未接线 |
| 21 | `SKILL_SHOWHP`(28) | `:431-442` | — | `:439` `Random(6) <= Level+3`（且目标非 null、`!ShowHp`） | `:438` `RM_DOOPENHEALTH`（目标） | `magic::power_show_hp_interval`（离线）；未接线 |
| 22 | `SKILL_BIGHEALLING`(29) | `:443-449` | — | `:447` `MagBigHealing(...)` | helper `:66` `RM_MAGHEALING`、`:71` `RM_ABILSEEHEALGAUGE` | `magic::power_heal_get_power_plus_sc2`（离线）；未接线 |
| 23 | `SKILL_SINSU`(30) | `:450-461` | — | `:457` `MagMakeSinSuSlave(...)` | helper：召唤落场 | **已接线**：`slave::cast_slave_spell`；尾已接 |
| 24 | `SKILL_ANGEL`(41) | `:462-473` | — | `:469` `MagMakeAngelSlave(...)` | helper：召唤落场 | **已接线**：`slave::cast_slave_spell`（同 #13/#23）；尾已接 |
| 25 | `SKILL_SHIELD`(31) | `:474-479` | — | `:477` `MagBubbleDefenceUp(...)`（`CharacterObject.cs:96-114`；`BubbleDefenceUP` 恒 0 ⇒ 恒 `true`） | helper：buff 落尾 | **已接线（效果体）**：`World::cast_shield_spell`；**尾未接** |
| 26 | `SKILL_KILLUNDEAD`(32) | `:480-488` | — | `:485` `MagTurnUndead(...)` | 无（helper 只改状态） | `magic_effect::mag_turn_undead`（纯函数）；未接线 |
| 27 | `SKILL_SNOWWIND`(33) | `:489-494` | — | `:492` `MagBigExplosion(...)` | helper `:1111` `RM_MAGSTRUCK` | 仅威力 helper（离线）；未接线 |
| 28 | `SKILL_UNAMYOUNSUL`(40) | `:495-523` | — | `:509`/`:514`/`:519` 三态计时器各自 `!= 0` 且 `Random(7)-(Level+1)<0` | 无 | 未建模 |
| 29 | `SKILL_WINDTEBO`(35) | `:524-529` | — | `:527` `MagWindTebo(...)` | `CharPushed` 内 `RM_PUSH` | 未建模 |
| 30 | `SKILL_MABE`(50) | `:530-536` | — | `:534` `MabMabe(...)` | helper `:1239/:1254` `RM_DELAYMAGIC`、`:1257` `RM_POISON(STONE)` | 未建模 |
| 31 | `SKILL_GROUPLIGHTENING`(42) | `:537-542` | **经 `ref` 置 `false`（`MagGroupLightening` `:984`）** | `:540` `MagGroupLightening(...)` 返回真 | helper `:987` `RM_MAGICFIRE`（自帧）、`:1004` `RM_DELAYMAGIC`、`:1012` `RM_10205` | 未建模；**null 目标 ⇒ `:987` NPE ⇒ 异常被吞 ⇒ 639、无尾**（见 §7） |
| 32 | `SKILL_GROUPAMYOUNSUL`(48) | `:543-548` | — | `:546` `MagGroupAmyounsul(...)` | helper `:921/:925` `RM_POISON` | 未建模；**MagID 48 两行**（首行=群体施毒术，第二行「气功波」被 `FindMagic` 遮蔽） |
| 33 | `SKILL_GROUPDEDING`(39) | `:549-554` | — | `:552` `MagGroupDeDing(...)` | helper `:968` `RM_DELAYMAGIC`、`:975` `RM_10205` | 未建模 |
| 34 | `SKILL_43`(43) | `:555-556` | — | **无赋值**（纯 `break`）⇒ 恒 `false` | 无 | 无效果体；**玩家路径不可达**（`PlayObject.Attack.cs:1253` 显式 case） |
| 35 | `SKILL_44`(44) | `:557-562` | — | `:560` `MagHbFireBall(...)` | helper `:1039` `RM_DELAYMAGIC`、`:1053` `RM_DELAYPUSHED` | 未建模 |
| 36 | `SKILL_45`(45) | `:563-588` | — | `:576` 在 `Random(10)>=AntiMagic` 内且 `Race>=Animal` | `:573` `RM_DELAYMAGIC`（self-delay） | `magic::power_lightening`（离线）；未接线 |
| 37 | `SKILL_46`(46) | `:589-594` | — | `:592` `MagMakeClone(...)`（恒 `true`）⇒ **恒真** | helper `PlayCloneObject` 构造尾 `RM_TURN` | **已接线**：`slave::cast_clone_spell`；尾已接 |
| 38 | `SKILL_47`(47) | `:595-600` | — | `:598` `MagBigExplosion(...)` | helper `:1111` `RM_MAGSTRUCK` | 仅威力 helper（离线）；未接线 |
| 39 | `SKILL_ENERGYREPULSOR`(37) | `:601-606` | — | `:604` `MagPushArround(...) > 0` | 无（同 #5） | **已接线**：与 8 同一块 `World::cast_firewind_spell`；尾已接 |
| 40 | `SKILL_49`(49) | `:607-609` | — | `:608` **无条件** `true` | 无 | 无效果体；未接线（Phase 1 可直接接） |
| 41 | `SKILL_UENHANCER`(36) | `:610-630` | — | `:626` **无条件** `true`（在 `:611` `boSpellFail=true` → `:620` 护身符 + 友方门内；`:627` 清 `boSpellFail`） | 无（只 `AttPowerUp`） | `magic_effect::power_defence_area`（离线）；未接线 |
| 42 | `SKILL_51`(51) | `:631-633` | — | `:632` **无条件** `true` | 无 | 无效果体；未接线（Phase 1） |
| 43 | `SKILL_52`(52) | `:634-636` | — | `:635` **无条件** `true` | 无 | 无效果体；未接线（Phase 1） |
| 44 | `SKILL_53`(53) | `:637-639` | — | `:638` **无条件** `true` | 无 | 无效果体；未接线（Phase 1） |
| 45 | `SKILL_54`(54) | `:640-642` | — | `:641` **无条件** `true` | 无 | 无效果体；未接线（Phase 1） |

### 2.1 前置门/护身符门的 case 归属（对尾的影响）

| 门 | 位置 | 影响的 case | 对尾的后果 |
|---|---|---|---|
| `IsWarrSkill` | `:151` | 3/4/7/12/25/26/27/34/38 | `return false`：无 `RM_SPELL`、无尾；3/4/7 另收 639（12/25/56/26/27/34/38 不走 `DoSpell`） |
| 距离门 | `:155` | 全部 | `return false`：无 `RM_SPELL`、无尾；**MP 已扣、53 已发**；调用方补 639 |
| 版本/魔号门 | `:167` | 全部（`MagicId > 80` 或老客户端） | `return false`：已发 `RM_SPELL`、**无尾**（⇒ 无 `RM_MAGICFIRE`）、调用方补 639 |
| 护身符门 `boSpellFail` | `:226`→`:254`（6）、`:322`→`:391`（13-19）、`:451`→`:459`（30）、`:463`→`:471`（41）、`:611`→`:627`（36） | 6/13/14/15/16/17/18/19/30/41/36 | 未过门 ⇒ `:644-647` 早退：**无 `RM_MAGICFIRE`、无训练尾**，调用方补 639 |

### 2.2 对任务描述的**实测订正**（必须按源码写，不按印象）

1. **改 `boSpellFire` 的不止 21**：`:406`（21 直接置 false）**与** `:984`（42 经 `ref` 在 `MagGroupLightening` 内
   置 false，随后 `:987` 自发 `RM_MAGICFIRE`）。⇒ 接 42 时尾必须传 `bo_spell_fire=false`，否则**双发**。
2. **`RM_MAGICFIRE` 的坐标/目标不是 `:159` 的入参**：多个 case 会就地改写 `nTargetX/nTargetY`
   （2 `:207-209`、36 `:614-616`、40 `:498-500` 改为施法者坐标；9 `:269`、10 `:281` 改为穿透终点）
   或改写 `targetObject`（1/5 `:191/:196/:201`、11 `:307/:312`、45 `:581/:586` 置 null；2/36/40 改为 `playObject`；
   `:160-163` 对死者置 null）。⇒ 尾部函数必须吃**case 之后的** `(tx, ty, target_actor_id)`。
3. `IsProperTarget(null)` 本身安全（`PlayObject.cs:1924-1926` 首句 `baseObject == null ⇒ false`），
   所以死亡归一后的 null 只会让分支跳过，不会 NPE；**唯一 NPE 是 42**（`:987` 直接读 `targetObject.ActorId`）。
4. 表格里 13/14/15/16/17/18/19 共用一个外块 `:315-393`；`boSpellFail` 只在 `:391` 统一清。
   ⇒ 接 14/15 的尾时，**必须先接护身符门**，否则「无护身符也会发 `RM_MAGICFIRE` 并练级」。
5. `MagIdx == MagicId`（`WorldServer.cs:1206`、`PlayDataStorage.Query.cs:359`）；但 `MagicConst.SKILL_CROSSMOON = 34`
   与魔表首行 `MagID=34`（解毒术）撞号 ⇒ 魔号 34 被 `IsWarrSkill` 当战士技在 `:151` 拒掉；
   魔表中**不存在 MagID 38**（`Idx=38` 那行的 `MagID` 是 48），所以 `SKILL_TWINBLADE(38)` 无行可命中。

---

## 3. default 支结论

### 3.1 switch 没有 `default:`

`awk 'NR>=172 && NR<=643 && /default:/'` 无输出；45 个 `case` 标签、38 个块，`:642` 之后直接 `:643 }`。
⇒ 魔号**无 case** 时，switch 体零执行，`boTrain` 保持 `:164` 的 `false`、`boSpellFail` 保持 `false`、
`boSpellFire` 保持 `:166` 的 `true`。

### 3.2 真实魔表里「无 case」的 MagID

数据源：`sql/mir2_data.sql`（`CREATE TABLE magics` `:43-65`，108 行 `INSERT` `:70-177`；content 冻结）。
`E:/MirServer/M2GameSvr/Envir/data.db` 头 8 字节 `6d 3a 5c b9 71 da 50 c9`，**不是**可直接读表的 SQLite/明文库
（同目录 `Mir200/`、`M2GameSvr-hook*/` 同形），故本件以受控 SQL dump 为准；**未连接/未查询在跑的 MySQL**。

- 魔表 `MagID` 全集（行数 108，唯一值 **95**）：`1-34, 36-37, 39-59, 66-69, 71-72, 74-80, 81-103, 500-501`
  （**无 35/38/60-65/70/73**；`38` 缺席正是 `SKILL_TWINBLADE` 无行可命中的原因）；
- 分派分类（`mir2-rs/tests/parity/m3/skill-probe/vectors/skills_registry.json`，`dospell_class`）：
  `implemented 29 / context_required 10 / no_effect_case 6 / no_switch_case 20 / warr_reject 8 / magicid_reject 35`；
- **`no_switch_case` = 20 行 / 18 个唯一 MagID**（`66`、`71` 各两行，`FindMagic` 首行胜）：

| MagID | 名称（`sql/mir2_data.sql` 行） | 玩家路径可达 | 备注 |
|---|---|---|---|
| 55 | 吸星狂杀（`:123`） | ✅ | `Job=0` 但 `IsWarrSkill` 只看魔号 ⇒ 进 `DoSpell` |
| 56 | 异形换位（`:124`） | ❌ | `PlayObject.Attack.cs:1152` 显式 case（`SKILL_REDBANWOL`）⇒ 不调 `DoSpell`；仅 `RobotPlayer.AutoSpell`（`RobotPlayer.cs:1870`）可达 |
| 57 | 回生术（`:125`） | ✅ | |
| 58 | 流星火雨（`:126`） | ✅ | |
| 59 | 噬血术（`:127`） | ✅ | |
| 66 | 四级魔法盾（`:128`、`:129` 两行） | ✅ | 首行胜（`Idx=66`） |
| 67 | 先天元力（`:130`） | ✅ | |
| 68 | 酒气护体（`:131`） | ✅ | |
| 69 | 倚天辟地（`:132`） | ✅ | |
| 71 | 四级召唤神兽（`:133`、`:134` 两行） | ✅ | 首行胜（`Idx=162`） |
| 72 | 召唤月灵（`:135`） | ✅ | |
| 74 | 逐日剑法（`:136`） | ✅ | |
| 75 | 护体神盾（`:137`） | ✅ | |
| 76 | 三绝杀（`:138`） | ✅ | |
| 77 | 双龙破（`:139`） | ✅ | |
| 78 | 虎啸诀（`:140`） | ✅ | |
| 79 | 追心刺（`:141`） | ✅ | |
| 80 | 凤舞祭（`:142`） | ✅ | |

> `no_effect_case`（**有 case、体内无副作用**）：43/49/51/52/53/54。行为上等价于「无 case」，
> 差别只有 `boTrain`：43 恒 false；49/51/52/53/54 恒 true。43 的玩家路径不可达（`PlayObject.Attack.cs:1253`
> 显式 case），其 `DoSpell` 路径仅机器人可达（见 G1 注）。

### 3.3 必然性判定

对任一无 case 的 MagID（§3.2 的 18 个，除 56 的玩家路径），走到共享尾时：

```text
boSpellFail = false（无 case ⇒ 无人置 true）      → :644 不早退
boSpellFire = true （无 case ⇒ 无人置 false）      → :648 发帧
boTrain     = false（无 case ⇒ 无人置 true）      → :659 门短路，零取数
```

⇒ **只要过 `:151` 战士门、`:155` 距离门、`:167` 版本/魔号门，就必然发「恰好一条」`RM_MAGICFIRE`**，
且**不抽任何随机数**、不改 `TranPoint`、不发 `RM_MAGIC_LVEXP`。“必然”的三个反例（全部由前置门负责，不在尾内）：

1. **`MagicId > 80`**：`:167` 在 switch/尾**之前** `return false` ⇒ 无 `RM_MAGICFIRE`
   （但 `:159` 的 `RM_SPELL` 与包装层的 MP/53 已经发生）。
2. **战士 9 魔号**（3/4/7/12/25/26/27/34/38）：3/4/7 不是 Attack.cs 显式 `case`，走 `default` 支；
   包装层 `PlayObject.Base.cs:1510` 的 `!IsWarrSkill` 为假（等价于 `MagicManager.DoSpell` 的 `:151` 战士门）
   ⇒ `result=false` ⇒ 调用方补 639；12/25/26/27/34/38 是 Attack.cs 显式 `case`（见下条）⇒ 根本不进 `DoSpell`。
3. **Attack.cs 的 8 条显式 case**（12/25/56/26/27/34/38/43）：不调用 `DoSpell` ⇒ 尾不存在。

外部证据（oracle 真身探针，见 `mir2-rs/crates/world/tests/dispatch_surface_vectors.rs`）：
`no_switch_case` 行的出帧账本恰为 `RM_SPELL;RM_HEALTHSPELLCHANGED;RM_MAGICFIRE`
（`10008:0:0:{id};10052:0:0:0;10120:257:0:0`），`magicid_reject` 行只有前两条
（`10008…;10052…`）⇒ 与 §3.3 的判定逐条一致。

---

## 4. 缺口清单

> 判据口径：全部走**生产入口** `handle_spell_cmd`；固定 `world.rng` 种子（`World::with_maps` 缺省 42，
> 测试可 `world.rng = RandomNumber::with_seed(s)`），断言**施法后连续若干抽**（≥3）的序列，而不是只看单抽。

### G0（前置缺口，必须先修）：`spell_version_gate` 无生产调用点

- 现状：`mir2_world::magic::spell_version_gate` 只有测试调用；`handle_spell_cmd` 从未调用。
- 后果：今天 81-103/500/501 **天然**没有 `RM_MAGICFIRE`（因为根本没有尾），一旦把尾接到 default 支
  而不同时接版本门，`MagicId > 80` 会**多发**一条 `RM_MAGICFIRE` ⇒ 新红。
- 需要的输入：`SoftVersionDateEx` 已有（`SessionGround::soft_version_date`）；`ClientTick` **没有**
  （`CM_LOGINNOTICEOK` 的 `nParam1`，`PlayObject.cs:1110-1114`，当前 game-svr 未处理该包）。
- 建议：本缺口单独成卡；在尾接线片里以 `magic_id > 80` 的**硬半边**先行拦截 + 登记
  `(SoftVersionDateEx==0 && ClientTick==0)` 半边未建模，或一次把 `CM_LOGINNOTICEOK` 接上。

### G1「必发尾而未发」（`RM_MAGICFIRE` 缺失）

| 分组 | 魔号 | 现状 | 可复现判据 |
|---|---|---|---|
| G1-a | 55/57/58/59/66/67/68/69/71/72/74/75/76/77/78/79/80 | 无任何支，尾未跑 | `SpellFrameRig` 学一个无 case 魔号（如 55）⇒ 观察者 `SM_SPELL` 恰 1，**紧跟 0 条 `SM_MAGICFIRE`**（应为 1） |
| G1-b | 14/15/31 | 效果体已接、**尾未接** | 开 `buff_tick`，15 过距离门并加 buff ⇒ 观察者 `SM_SPELL` 1 条、`SM_MAGICFIRE` 0 条（应为 1）；且 `TranPoint` 无变化 |
| G1-c | 1/2/5/6/9/10/11/13/16/18/19/22/23/24/28/29/32/33/36/39/40/42/44/45/47/48/49/50/51/52/53/54 | 无生产支（49/51-54 无效果体；其余有效果体但未接线） | 同 G1-a；对 42 还要额外断言 `SM_MAGICFIRE` **恰 1**（不是 2） |

> 43 与 56 同口径：`PlayObject.Attack.cs:1253` / `:1152` 显式 `case` ⇒ 玩家路径永不调用 `DoSpell`，
> 不列为「必发尾而未发」。魔号 35（`SKILL_WINDTEBO`）在 switch 有 case（`:524-529`），Rust 无任何支；
> 但它不在 `sql/mir2_data.sql` 的 MagID 全集（`WorldServer.cs:1201-1211` 的 `FindMagic` 返回 null
> ⇒ 学不到 ⇒ 玩家路径不可达），故不计入 G1/G2。

### G2「训练尾抽数缺失」

| 分组 | 魔号 | 缺的抽 | 可复现判据 |
|---|---|---|---|
| G2-a | 49/51/52/53/54 | 尾 `Random(3)+1`（恒 `boTrain=true`） | 固定种子、`Level<3`、`Abil.Level >= TrainLevel[0]`：施法后连续 3 抽 ≠ 未施法对照（当前**完全相等** ⇒ 红） |
| G2-b | 14/15/31 | 尾 `Random(3)+1`（条件真时） | 同 G2-a，但先满足 `cast_defence_area_spell(...) > 0` / `MagBubbleDefenceUp` 恒真 |
| G2-c | 上述 G1-c 中所有 `boTrain` 可为真的 case（1/2/5/6/9/10/11/13/16/18/19/22/23/24/28/29/32/33/36/39/40/42/44/45/47/48/50） | 先缺 case 内取数、再缺尾取数（35 因魔表无行不可达，不计） | 必须**等效果体**；单独补尾只会把一次抽取插到错误的流位置（见 §5.3） |

### G3「`RM_MAGIC_LVEXP` 缺失」

- 覆盖集合 = G2 中「抽数后 `CheckMagicLevelUp` 返回 false」或「升级成功」的实例；
  升级路径的 `RM_MAGIC_LVEXP` 由 `CheckMagicLevelUp` 内的 `UpdateDelayMsg(..., 800)` 发出
  （`PlayObject.cs:5216`）；只有 `CheckMagicLevelUp` 返回 false 时才是
  `SendSelfDelayMsg(..., 1000)`（`MagicManager.cs:666`）。两处帧内容相同（`0, MagicId, Level, TranPoint, ""`），
  仅延迟不同；本仓 `send_self_frame` 均立即发，延迟面属既有全局登记。
- 可复现判据：把 `TranPoint` 预置到 `MaxTrain[Level] - 1` 再施法（49 或 14/15/31）⇒ 应收到
  `SM_MAGIC_LVEXP(640)` 且 `Level+1`、`TranPoint` 扣减；当前**既无帧也无落库**（`saves[...].record.data.magic` 不变）。

### G4「尾帧参数/收件人」缺口（与 G1 同时修）

1. **死亡归一**：17/30/41/20/46 的 `target_id` 只做了「命中」判定，未做 `Death` 过滤
   （8/21/37 已做）。死者为目标时：oracle `:160-163` ⇒ `targetObject=null` ⇒ `RM_MAGICFIRE` 的 ActorId=0；
   Rust 会带死者 id。⇒ 统一在 `spell_snap_target` 后做一次 `filter(|t| !t.death)`。
2. **21 之外的自帧**：42 在 helper 内自发 `RM_MAGICFIRE`（`:987`）⇒ 尾必须 `bo_spell_fire=false`。
3. **坐标**：尾帧必须用 case 后的 `(tx, ty)`（§2.2-2）。

---

## 5. 实现方案

### 5.1 最小改动（一个尾函数 + 一个分派表）

当前尾逻辑共 **4 处** `do_spell_tail` 调用点（`bins/game-svr/src/main.rs` 1 处：8/21/37；
`bins/game-svr/src/slave.rs` 3 处：17/30/41、20、46；形态相同：
`do_spell_tail` → 回写 `saves` → `sm_magicfire` → `sm_magic_lvexp`）。建议：

```text
// bins/game-svr/src/main.rs（宿主私有）
struct SpellTailCtx<'a> {
    chr_name: &'a str,
    magic: &'a mir2_data::MagicInfo,     // effect_type / effect / train_lv / train_level / max_train
    magic_id: u16,
    magic_level: u8,
    tran_point: i32,
    caster_abil_level: u8,
    bo_spell_fail: bool,
    bo_spell_fire: bool,
    bo_train: bool,
    fast_train: bool,                    // 未装载 ⇒ 恒 false（登记）
    tx: i32, ty: i32,
    target_actor_id: i32,                // 已做死亡归一
}

/// 返回 oracle `DoSpell` 的 bool（`bo_spell_fail ⇒ false`）。
/// 顺序严格：`do_spell_tail`（含唯一一次 Random(3)+1）→ 回写 saves → `sm_magicfire`
/// → `sm_magic_lvexp`。禁止在中间插任何 `world.rng` 取数。
fn finish_spell_tail(ctx: SpellTailCtx<'_>, world: &mut World, /* online/saves/gates/… */) -> bool
```

并加一个**唯一**的分派判定，供「无条件 `RM_SPELL` 块」与「尾」共用，避免两处集合漂移：

```text
enum SpellArm {
    // Attack.cs `switch (UserMagic.MagIdx)` 的 8 个显式 case（12/25/56/26/27/34/38/43）：
    // 不进 `default` ⇒ 永不调用 `DoSpell`（尾与 639 都不存在）。
    ExplicitCase,
    // IsWarrSkill 命中且非显式 case（3/4/7）：走 `default`，但战士门早退。
    WarrReject,
    // 已接效果体且尾已接。
    Firewind8, SpaceMove21, EnergyRepulsor37, Slave173041, Tamming20, Clone46,
    // 已接效果体、尾未接（P2）。
    BuffDefence1415, BuffShield31,
    // 无 case（严格限魔号 55-80，除 56）：switch 体零执行 ⇒ 必发尾、零取数（P1）。
    NoSwitchCase,
    // 有 case、体内无副作用（49/51/52/53/54）⇒ 必发尾、恒 boTrain=true（P1）。
    // 43 不在此臂：它是显式 case（ExplicitCase），玩家路径不可达。
    NoEffect49_51_52_53_54,
    // 其余有 case 的魔号（P3）：效果体未实体化，**不接尾**
    // （含 35；35 无魔表行不可达。>80 的 81-103/500/501 不进 NoSwitchCase，由版本门 G0 拦截）。
    DeferredCase,
}
fn spell_arm(magic_id: u16) -> SpellArm
```

不改判据、只搬调用点的重构本身是 L；但它触碰 **RNG 抽取位置**，按 `AGENTS.md` 判据分级属 **R**
（审查时写 `window_required`，并跑 §6 的连续抽数牙）。

### 5.2 default 支的落点（与现有块不重复）

在 `handle_spell_cmd` 的**所有效果体块之后、`send_good_tick` 之前**收口：

```text
// `default_arm` = `magicwire::takes_default_dir(id)`（排除 12/25/56/26/27/34/38/43）；
// `!is_warr_skill` 排除 3/4/7。尾只接在这两个门内、且只接 P1 的两条臂。
if default_arm
    && !magic::is_warr_skill(i32::from(id))
    && matches!(spell_arm(id), SpellArm::NoSwitchCase | SpellArm::NoEffect49_51_52_53_54)
{
    // bo_spell_fail = false；bo_spell_fire = true；
    // bo_train = matches!(id, 49|51|52|53|54)（NoSwitchCase 恒 false）
    // target_actor_id / (tx,ty) = 头顶 `spell_snap_target` 已算好的值（需把它提升到块外）
    let _ = finish_spell_tail(...);
}
```

要点：
1. `bo_spell_fail=false` / `bo_spell_fire=true` 对这两类**是定值**（§3.3），不需要 case 提供。
2. 49/51/52/53/54 恒 `bo_train=true`（`NoSwitchCase` 恒 `false`）—— 这是唯一需要的「case 语义」，
   用 `match` 写死即可。43 不在尾接线集合内：它是 `ExplicitCase`，`default_arm` 为假 ⇒ 尾块不可达（死代码）。
3. 必须放在版本门（G0）**之后**；且不能与 8/21/37、17/30/41、20、46 的块重叠（用 `spell_arm` 保证互斥）。
4. 对 14/15：在各自块内 `let bo_train = cast_defence_area_spell(...) > 0;`，再调 `finish_spell_tail`；
   14/15 还必须补 `magic::check_amulet`/`use_amulet` + `bo_spell_fail`（见 §7-10 的夹具影响）。
   对 31：**`bo_train` 恒为 `true`**——`World::cast_shield_spell` 返回的是 `n_sec: i32`，而 oracle
   `MagBubbleDefenceUp`（`CharacterObject.cs:96-114`）在 `BubbleDefenceUP` 恒 0 时恒返回 `true`；
   按 `n_sec > 0` 判会在 `n_sec == 0` 时漏一次训练抽。

### 5.3 分期

| 期 | 范围 | 为什么可以/必须等 | tier |
|---|---|---|---|
| **P0** | G0 版本门（至少 `magic_id > 80` 半边）+ `spell_snap_target`/死亡归一提升到块外 | 不先拦 >80，尾会多发帧；不提升坐标，尾帧参数不可得 | R |
| **P1** | default 支：18 个无 case 魔号（玩家路径 17 个）+ `no_effect_case` 49/51/52/53/54 | 这些 case **零取数** ⇒ 补尾不会打乱任何已绿的流；49/51-54 的尾抽是 oracle 在该魔号上的唯一一次抽取。43 不进本批：`PlayObject.Attack.cs:1253` 显式 case ⇒ 玩家路径不调 `DoSpell` | R（含 RNG） |
| **P2** | 14/15/31 的尾 + 14/15 的护身符门 | 效果体已在，只差 `bo_train` 与尾；改了现有 A-5 牙的前置（见 §7-10） | R |
| **P3** | 其余「有实体效果」的 case | **必须等 case 实体化**：oracle 在这些魔号上先抽 case 内 N 次、再抽尾 1 次；只补尾会在错误位置插一次抽取，等于把「缺 N 次」变成「缺 N 次 + 错位」 | R |

> 「必须等 case 实体化」的准确口径：45 个 case 标签里，P1 覆盖 49/51/52/53/54（5 条无效果体；43 是
> `PlayObject.Attack.cs:1253` 显式 case，玩家路径不调 `DoSpell`，不计入尾接线），P2 覆盖 14/15/31
> （3 条已建效果体）；8/21/37/17/30/41/20/46 的效果体与尾都已接，属**已完成**。**其余 29 条**都要等
> 效果体（其中 35 因无魔表行不可达；43 无效果体、玩家路径不可达，仅保留分类），不进 P1/P2。

---

## 6. 机判验收 + 改坏必红（≥4 条）

全部走生产入口 `handle_spell_cmd`；夹具沿用 `SpellFrameRig` / `effect_rig`；种子固定。
「连续若干抽」= 施法后从同一 `world.rng` 再取 ≥3 个数与对照流比较（对照流 = 同种子、不施法）。

| # | 断言（生产入口） | 改坏什么 ⇒ 哪个断言红 |
|---|---|---|
| J1 | **default 支必发尾**：魔号 55（无 case）⇒ 观察者序列含 `SM_SPELL` 恰 1 条，**且其后的 `SM_MAGICFIRE` 恰 1 条**、ActorId=0（无目标）；施法者自己那条连接收 `SM_MAGICFIRE`（`RM_MAGICFIRE` 处理器无自守卫）、不收 `SM_SPELL` | 删掉 default 支的 `finish_spell_tail` 调用 ⇒ `SM_MAGICFIRE` 计数 0 ⇒ 红 |
| J2 | **default 支零取数**：魔号 55/57/66/80 各施法一次后连续 3 抽 == 未施法对照流的连续 3 抽 | 把 `bo_train` 传成 `true`（或无条件走训练尾）⇒ 多抽一次 ⇒ 3 抽序列错位 ⇒ 红 |
| J3 | **49/51/52/53/54 的尾抽恰一次且位置正确**：`Level<3`、`Abil.Level >= TrainLevel[0]` 施法 49 后，`TranPoint` 增量 ∈ [1,3]，且随后 3 抽 == 「对照流跳过 1 抽」；`SM_MAGIC_LVEXP` 只在调用 `CheckMagicLevelUp` 后按 oracle 条件出现 | 不传 `bo_train=true` ⇒ `TranPoint` 不变、3 抽 == 对照（错位 0）⇒ 红；重复调用尾 ⇒ `TranPoint` 增量可能 >3 且 3 抽跳过 2 ⇒ 红 |
| J4 | **21/42 的 `boSpellFire=false`**：21 施法后观察者 `SM_MAGICFIRE` 恰 1（来自 case，`bo_spell_fire=false`）；42 建模后同断言 | 把尾的 `bo_spell_fire` 传 `true` ⇒ 21 变 2 条 ⇒ 红；42 变 2 条 ⇒ 红 |
| J5 | **14/15 护身符门 + 尾**：无护身符 ⇒ 目标**不得**获得 `DefencePower`、观察者 `SM_MAGICFIRE` 0 条、调用方 `+GD` 仍在（`ClientSpellXY` 恒 `result=true`）；有护身符（`use_items[SLOT_ARMRING_L]` 装 `index=1`/`dura=600`，且 catalog `get(1)` 返回 `std_mode=25`/`shape=5` 模板）⇒ buff 落地且 `SM_MAGICFIRE` 1 条 | 去掉 `bo_spell_fail` 分支 ⇒ 无护身符也发 `SM_MAGICFIRE`/加 buff ⇒ 红；只加尾不加护身符门 ⇒ 同红 |
| J6 | **版本门位置**：魔号 81（`>80`）⇒ `SM_SPELL` 仍 1 条、`SM_MAGICFIRE` **0 条**、`+GD` 仍在；魔号 55 ⇒ `SM_MAGICFIRE` 1 条 | 把尾提到版本门之前 ⇒ 81 获得 `SM_MAGICFIRE` ⇒ 红；把版本门提到 `:159` 位置之前（即 RM_SPELL 之前）⇒ 81 的 `SM_SPELL` 变 0 ⇒ 红 |
| J7 | **死亡归一**：目标已死 + 魔号 20 ⇒ 观察者 `SM_MAGICFIRE` 的 ActorId **= 0**，且无 `MagTamming` 取数（连续 3 抽 == 对照流）；魔号 17/30/41/46 同 ActorId=0 | 去掉 `filter(!death)` ⇒ ActorId = 死者 id ⇒ 红 |
| J8 | **RNG 站点归属**：`spell_tail_rng_trace` 的 trace 里尾抽恰 1 行且落在 `crates/world/src/magic_effect.rs`，不得出现 `crates/world/src/magic.rs`（`rng_ledger_diff.py` 的 `magic_effect` 组口径） | 把 `spell_train_points` 挪回 `magic.rs` ⇒ trace 断言红 |

> 现有可复用的机器资产：`mir2-rs/crates/world/tests/dispatch_surface_vectors.rs`（no_switch_case/magicid_reject
> 真身账本）、`mir2-rs/crates/world/tests/residual_case_vectors.rs`（43/49/51-54 的端到端 + 三条红检）、
> `mir2-rs/crates/world/tests/combat_skill_vectors.rs::dospell_classes_are_fully_registered`（108 行分类门禁）、
> `mir2-rs/bins/game-svr/tests/spell_tail_rng_trace.rs`（尾抽站点）。

---

## 7. 未决 / 风险

1. **`FastTrain` 未装载**：`PlayObject.FastTrain`（快速修炼项链，`PlayObject.cs:2168`）本仓无字段 ⇒ 所有
   `finish_spell_tail` 调用点恒传 `false`（与 `slave.rs` 现有三处同口径）。影响：`TrainSkill` 的 ×3 分支不可达，
   熟练度增量上界 3 而非 9。属已登记偏差，不阻塞尾接线。
2. **`targerActors` / 死亡归一的 target**：`:404` 的 `targerActors` 与 `:650/:656` 的 ActorId 都读
   `:160-163` 归一后的 `targetObject`。Rust 需在**一处**归一（`spell_snap_target` 之后），否则 4 处各写一遍
   必然漂移（当前 8/21/37 已过滤、17/30/41/20/46 未过滤 ⇒ 已可观测差异，见 J7）。
3. **`RM_MAGICFIRE` 收件人集合**：`SendRefMsg` 的接收侧 `PlayObject.Message.cs:1788-1793` **无**自守卫
   ⇒ 与 `RM_SPELL`（`:1455` 有 `processMsg.ActorId != ActorId`）**不同**：`SM_MAGICFIRE` 含施法者自己。
   Rust 必须用 `broadcast_ref_msg_with_self`；用 `broadcast_ref_msg` 会漏自己那条连接。将来若有人「统一」两者，
   J1/J4 会红。
4. **与在飞 `distfail`（距离门失败 639）的先后关系**：
   `:155` 早于 `:159`/switch/尾 ⇒ 距离门失败**没有** `RM_SPELL`、没有尾；但包装层已扣 MP、已发 53
   （`PlayObject.Base.cs:1512-1520` 在 `MagicManager.DoSpell` 之前）。调用方 `PlayObject.Attack.cs:1278-1281`
   补一条 `RM_MAGICFIREFAIL`（⇒ `SM_MAGICFIRE_FAIL(639)`）。⇒ `distfail` 卡应保证：**先 MP/53，再 639，无尾**。
   本尾片不能把距离门挪到包装层之后，否则 MP 语义变。
5. **与 `18c7ffe56`（MP 包装层）的时序**：已接的 MP 块位于「`default` 支方向设定」之后、无条件 `RM_SPELL` 块
   之前，与 oracle 一致（`:1519-1520` 先于 `:159`）。尾接线**不得**前移或后移该块；尾必然在 switch 之后，
   即 53 帧 → `SM_SPELL` → case 帧 → `SM_MAGICFIRE` → `SM_MAGIC_LVEXP`。
6. **42 的 null 目标 NPE**：`:984` 先置 `boSpellFire=false`，`:987` 直接读 `targetObject.ActorId`。
   无目标施放 42 ⇒ `NullReferenceException` ⇒ `PlayObject.Base.cs:1525-1529` 吞掉 ⇒ `DoSpell` 返回 false
   ⇒ 调用方发 639；MP 已扣、`RM_SPELL` 已发、**无 `RM_MAGICFIRE`**。接 42 时必须显式复刻这一「异常即失败」路径，
   不能按「null ⇒ ActorId 0」写（那会多发一条 `RM_MAGICFIRE`）。
7. **`RM_MAGIC_LVEXP` 的延迟**：升级路径由 `CheckMagicLevelUp` 内的 `UpdateDelayMsg(..., 800)`
   （`PlayObject.cs:5216`）发出，只有未升级时才是 `SendSelfDelayMsg(..., 1000)`（`MagicManager.cs:666`）。
   两处帧内容/字段相同、仅延迟不同；本仓 `send_self_frame` 均立即发，时序不建模（既有全局登记）。
   若做帧序对拍，需把该帧视为「本 tick 末/延迟队列」，不能与紧邻帧做严格相邻断言。
8. **13 个重复 `MagID`（48、66、71、88、89、90、91、92、93、94、96、97、98；108 行 / 95 个唯一值）**：
   `FindMagic` 首行胜；`spell_arm`/`finish_spell_tail` 只吃 `magic_id`，不受影响；但
   `magics::MagicTable::get` 的装载顺序必须保持 `Idx` 升序，否则 `train_level`/`effect` 会取到被遮蔽行
   （`magics::MagicTable` 注释同写 13 个）。
9. **`boSpellFail` 后的 639 尚未接（除 MP 支外）**：`slave::cast_slave_spell` 已给出 `accepted`，
   但 `handle_spell_cmd` 里 `let _ = outcome.accepted;`（17/30/41/20/46），没有发 `sm_magicfire_fail`。
   尾接线会把「`boSpellFail ⇒ false`」变成显式返回值，正好是同一条链的落点；建议与 `distfail` 卡合流，
   避免两卡各写一次 639。
10. **14/15 现有 A-5 牙无护身符**：`a5_spell_rig`（及其调用点构造的 `ground_sessions`/`catalog`）既无
    `ArmRingl` 槽也无 catalog 模板。一旦补 `CheckAmulet`（§5.3 P2），
    `spell_cmd_a5_defence_area_buffs_target_in_range`（魔号 15）会因 `boSpellFail` 变红。**这是牙错不是实现错**：
    夹具必须同时补 **use_items 槽 + catalog 模板**——`amulet_slot` 要 `use_items[slot]` 非空且
    `ItemCatalog::get(index)` 返回模板（`CheckAmulet` 还要求 `Shape==5`、`Round(Dura/100)>=nCount`）；
    只加 `ArmRingl Shape=5 Dura>=100` 仍会因 `ItemCatalog::get` 返回 None 而失败。可照 `SpellFrameRig` 的
    布置：`index=1`、`std_mode=25`、`shape=5`、`dura=600`。并在 patch 说明里写明「牙的布置修正」。
11. **`buff_tick_enabled` 缺省关闭**：14/15/31 的尾只在 buff tick 开启的测试/场景里可观测；默认 golden 不受影响。

---

## 8. 证据与复现命令（只读）

```sh
# 1) switch 无 default；case 标签共 52 行 = 外 switch 45 + :326 内嵌 switch 的 7 个重复标签
#    （:328/:352/:359/:366/:372/:378/:384）；另有各 break;
awk 'NR>=172 && NR<=643 && (/^ *case MagicConst/ || /^ *break;$/ || /default:/)' \
  src/M2Server/Magic/MagicManager.cs

# 2) 共享尾逐行（:644-670）与前置门（:145-172）
awk 'NR>=144 && NR<=172 {printf "%d\t%s\n", NR, $0}' src/M2Server/Magic/MagicManager.cs
awk 'NR>=640 && NR<=672 {printf "%d\t%s\n", NR, $0}' src/M2Server/Magic/MagicManager.cs

# 3) 调用方：Attack.cs 的 8 条显式 case + default 支 + RM_MAGICFIREFAIL
awk 'NR>=1110 && NR<=1290 && (/switch \(UserMagic.MagIdx\)/ || /case MagicConst/ ||
     /default:/ || /DoSpell/ || /RM_MAGICFIREFAIL/ || /CretInNearXy/)' \
  src/M2Server/Player/PlayObject.Attack.cs

# 4) 包装层（MP/53 先于 RM_SPELL）与 GetSpellPoint
awk 'NR>=1505 && NR<=1531 {printf "%d\t%s\n", NR, $0}' src/M2Server/Player/PlayObject.Base.cs
awk 'NR>=1369 && NR<=1372 {printf "%d\t%s\n", NR, $0}' src/M2Server/Player/PlayObject.Base.cs

# 5) 魔表 108 行 + MagID 列
grep -c "INSERT INTO \`magics\`" sql/mir2_data.sql
grep -oP "INSERT INTO \`magics\` VALUES \(\K[0-9]+, [0-9]+" sql/mir2_data.sql | awk -F', ' '{print $2}' | sort -n | uniq | tr '\n' ' '

# 6) 分类与 20 行 no_switch_case（唯一 id 18 个）
python -c "import json;d=json.load(open('mir2-rs/tests/parity/m3/skill-probe/vectors/skills_registry.json',encoding='utf-8'));print(sorted({x['magic_id'] for x in d['skills'] if x['dospell_class']=='no_switch_case'}))"
```

关键输出摘录：

- `MagicManager.cs:167` = `if (playObject.SoftVersionDateEx == 0 && playObject.ClientTick == 0 || userMagic.Magic.MagicId > 80)`；
  `:640-642` 是最后一个 case；`:643 }`；`:644-670` 尾；无 `default:`。
- `PlayObject.Attack.cs:1118` switch / `:1269` default / `:1278` DoSpell / `:1280` RM_MAGICFIREFAIL。
- `sql/mir2_data.sql`：108 行 `INSERT INTO magics`（`:70-177`），`MagID` 唯一值 **95** 个（13 个重复：48/66/71/88-94/96-98；82 个只出现一次）。
- `skills_registry.json`：`no_switch_case` 20 行 = id `{55,56,57,58,59,66,67,68,69,71,72,74,75,76,77,78,79,80}`
  （66/71 各两行）；类别计数 `implemented 29 / context_required 10 / no_effect_case 6 / no_switch_case 20 /
  warr_reject 8 / magicid_reject 35`。
- oracle 真身账本（`dispatch_surface_vectors` 的 `expected_frames`）：no_switch_case = `SM_SPELL;53;RM_MAGICFIRE`；
  magicid_reject = `SM_SPELL;53`。

---

## 9. 残留物清单 / 只读声明

- 新增文件（唯一产物）：`mir2-rs/tests/parity/m3/design-spell-shared-tail-20261017.md`。
- worktree：`/e/tmp/impl-sharedtail2`（与任务给定的 `git worktree add` 路径一致；patch 从此处 `git diff --cached` 导出）。
- 未修改 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`、`E:/MirServer/**`；
  未启动任何服务/客户端/假人；未占用端口；未连接/写入 MySQL；未删除任何文件。
- 只读打开过：`src/**`（oracle 源）、`sql/mir2_data.sql`、`mir2-rs/**`（Rust 现状与测试向量）。
  `E:/MirServer/M2GameSvr/Envir/data.db` 只做了 8 字节头识别（非表存储），未解析、未复制。
