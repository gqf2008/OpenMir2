# openmir2-attack-yedo-batch-b

> 状态：**草案（2026-11-20，待 opc-19 审定后排期）**。来源 = 攻击批拆分（A/B/C）里的 **B**，
> 范围严格限定为 A 卡 §6.1 自交代中「归 B」的三项 + A 留下的 `rm_action_ident` case 9 偏差。
> **本卡不写代码**，只给范围、oracle 锚、帧面口径、判据与代价。

## 0. 范围（且仅此）

| # | 项 | oracle 入口 |
| --- | --- | --- |
| ① | **技能熟练度/升级**：`CheckSkillProficiency` / `TrainSkill` / `CheckMagicLevelUp`，以及 A 卡登记为「归 B」的 **`MagicArr` 等级时点**（A 的攻杀重置要用训练后的 `Level`） | `PlayObject.Attack.cs:200-213`、`PlayObject.cs:4393`、`:5198` |
| ② | **`PowerHit` 消费**（模式 3）：`GetAttackPowerHit` 的 `nPower += HitPlus` + 清零 + `canHit=true`，`GetHitMode` 的 `RM_SPELL2` | `PlayObject.Attack.cs:586-590`、`:94-97` |
| ③ | **`TwinHitSkill` 与 `rm_action_ident` case 9**：oracle `case 9` 只有在 `TwinHitSkill` 为真时才发 `RM_TWINHIT`（现 Rust 无条件发），连同它的置位/超时与 `GetAttackPowerHit` 模式 9 的消费 | `PlayObject.Attack.cs:124-128`、`:597-601`、`PlayObject.Base.cs:666-672`、`PlayObject.Message.cs:60-64` |

**不做**（明确边界）：
- **C 项**（`SkillAttackDamage` 次目标 / 模式 4/5/8/12 的 MP 门与邻居扫描）——等 `IsProperTarget`/安全区口径裁定，另卡。
- **657 帧（`SM_CHARSTATUSCHANGED`）**——已派 `2f`（T-290 升级链末）。本卡**不碰**该函数区、不改 657 的任何发射点。
- `FireHitSkill`（模式 7）：见 §6 决策点 D-2，暂**不在**本卡（除非审阅要求并入）。

## 1. oracle 锚（§39：名字 + 定义位置 + 读/写）

### 1.1 训练与升级（本项 ①）

| 符号 | 定义位置 | 读 | 写 / 发 |
| --- | --- | --- | --- |
| `TrainSkill(UserMagic, int)` | `src/M2Server/Player/PlayObject.cs:4393-4400` | `FastTrain`（`:4395`） | `userMagic.TranPoint += nTranPoint`（`FastTrain` 时 `nTranPoint*3`） |
| `CheckMagicLevelUp(UserMagic) -> bool` | `PlayObject.cs:5198-5226` | `Level`、`Magic.TrainLv`、`Magic.MaxTrain[nLevel]`（`nLevel = (Level<4 && TrainLv>=Level) ? Level : 0`，`:5202-5209`）、`TranPoint` | `TranPoint -= MaxTrain[nLevel]`、`Level++`（`:5214-5215`）、`UpdateDelayMsg(RM_MAGIC_LVEXP, 0, MagicId, Level, TranPoint, "", 800)`（`:5216`）、`CheckSeeHealGauge`（`:5217`；定义 `:5231-5240`，只对 `MagicId==28 && Level>=2` 置 `AbilSeeHealGauge`） |
| `CheckSkillProficiency(int, UserMagic)` | `PlayObject.Attack.cs:200-213` | `userMagic.Level < 3`、`Magic.TrainLevel[Level] <= Abil.Level`（`:204`） | 调 `TrainSkill`；`!CheckMagicLevelUp(...)` ⇒ `SendSelfDelayMsg(RM_MAGIC_LVEXP, 0, MagicId, Level, TranPoint, "", 3000)`（`:209`） |
| `GetAttackMagic(int magicId)` | `PlayObject.cs:5399-5402` | — | 返回 `MagicArr[magicId]` |

**`MagicArr` 与 `MagicList` 是同一批对象**：`RecalcHitSpeed` 的 `MagicArr[userMagic.MagIdx] = userMagic`（`PlayObject.cs:3684`）是**引用赋值** ⇒ 经 `MagicArr` 升级 = `MagicList` 里那件技能升级（存档同源）。

**攻击路径的四个调用点（按执行顺序）**

1. `AttackSuccess`（`PlayObject.Attack.cs:215-336`）内：
   - `:241-249` 精神力战法：`MagicArr[SKILL_ILKWANG] != null && Level < 3` ⇒ `CheckSkillProficiency(Random(3)+1, magic)`。
     **抽数在实参位置**，先抽后进门（`CheckSkillProficiency` 内部才判 `TrainLevel[Level] <= Abil.Level`）。
   - `:250-260` 攻杀剑法：`canHit && MagicArr[SKILL_YEDO] != null && Level < 3` ⇒ `CheckSkillProficiency(Random(3)+1, magic)`。
     **`canHit` 只有模式 3/7/9 命中消费时为真**（见 §1.2）⇒ 这正是本卡 ① 与 ② 的交汇点。
   - `:264-290` 命中模式附加：模式 4/5/12/7/8/9 ⇒ `CheckSkillProficiency(1, magic)`（**常量 1，不抽**）。
2. `:337` `TrainCurrentSkill(wHitMode)`（定义 `:353-560+`）：**每次命中都跑**，逐技能带门
   （`Level < Magic.TrainLv && Magic.TrainLevel[Level] <= Abil.Level`）⇒ `TrainSkill(..., Random(3)+1)`：
   - `:362-375` 基本剑法（`SKILL_ONESWORD`）、`:376-389` 精神力（`SKILL_ILKWANG`）——**无条件各查一次**；
   - 之后按 `wHitMode` 的 case（3 攻杀 / 4 刺杀 / 5 半月 / 12 红半月 / 7 烈火 / 8 逐日 / 9 狂风斩 …）各一条。

**时点（审阅点名要的）**：训练与升级都是**攻击时刻同步发生**的（`AttackSuccess` 在 `AttackDir` 内、
`AttackDir` 在 `ClientHitXY` 内），**不是** drain 时刻。延迟的只有**帧**：升级走 `UpdateDelayMsg(…, 800)`，
未升级走 `SendSelfDelayMsg(…, 3000)`。`UpdateDelayMsg`（`BaseObject.Message.cs:257-278`）的语义是
**先按 `(wIdent, nParam1)` 摘掉队列里已有的同条再重新入队**（即"覆盖旧熟练度帧"），不是"再发一条"。

⇒ 直接后果（A 卡那条「归 B」的偏差）：**同一击里的攻杀重置**（`PlayObject.Attack.cs:942-956`，在
`AttackSuccess` **之后**跑）读到的是**已升级后**的 `Level` ⇒ `7-L` 用的是新等级。

### 1.1b 时点钉死：训练在**攻击时刻同步**发生，只有帧延迟（逐行）

调用链（全部同步、无入队、无 tick 边界）：

| 步 | 位置 | 做什么 |
| --- | --- | --- |
| 1 | `PlayObject.Attack.cs:908-939`（`ClientHitXY` 的 `switch (wIdent)`） | 按命令 ident 直接调 `AttackDir(null, wHitMode, nDir)`（**同步调用**，不是入队） |
| 2 | `PlayObject.Attack.cs:24`（`AttackDir` 定义） | `.72` `canHit=false` → `.73` `GetAttackPowerHit(...)`（**模式 3/7/9 在此消费 `PowerHit`/`TwinHitSkill`**）→ `.74` `SkillAttackDamage` → `.75` `AttackDir(target, nPower, nDir)`（`_Attack`，伤害结算）→ `.76` `SendAttackMsg` → `.77` `AttackSuccess(...)` |
| 3 | `PlayObject.Attack.cs:215-338`（`AttackSuccess`） | `.241-260` 两次 `Random(3)+1` 训练（精神力/攻杀，门内先抽）→ `.264-290` 按模式常量 `1` 训练 → **`.337 TrainCurrentSkill(wHitMode)`**（每次命中都跑，逐技能 `Level < TrainLv && TrainLevel[Level] <= Abil.Level` 再抽 `Random(3)+1`） |
| 4 | 同上，`TrainSkill`→`CheckMagicLevelUp` | 同步改 `MagicList` 那件技能的 `TranPoint`/`Level`；升级时 `UpdateDelayMsg(RM_MAGIC_LVEXP,…,800)`（`PlayObject.cs:5216`），否则调用方 `SendSelfDelayMsg(…,3000)`（`PlayObject.Attack.cs:209`） |
| 5 | `PlayObject.Attack.cs:942-956`（回到 `ClientHitXY`，`switch` 之后） | 攻杀计数块读 **`MagicArr[SKILL_YEDO].Level`** ⇒ 读到的是第 4 步**已升级后**的值 |

**结论（三句话）**
1. **不是 drain 时刻**：`TrainSkill`/`CheckMagicLevelUp` 在第 1 步的同一次同步调用栈里执行完，攻击返回时
   等级/熟练点已经是新值；`drain`/延时队列里只有**帧**（640，800ms 或 3000ms）。
2. **同击内可见**：第 5 步的攻杀重置用新 `Level` ⇒ `7-L` 用新等级（A 卡「归 B」的那条偏差就是它）。
3. **帧序**：640 帧在攻击时刻**入队**，到点才出；升级走 `UpdateDelayMsg`（先摘掉同 `(ident, MagicId)`
   的旧条再入队，`BaseObject.Message.cs:257-278`），未升级走 `SendSelfDelayMsg`（`:188-206`，优先级
   `Lowest`）。⇒ 同一击内同一魔号只留**一条** 640；跨击则后一条**覆盖**前一条的等待时间（不是叠加两条）。

### 1.2 `PowerHit` 消费（本项 ②）

| 符号 | 定义位置 | 读 | 写 |
| --- | --- | --- | --- |
| `GetAttackPowerHit(short, int, IActor, ref bool) -> int` | `PlayObject.Attack.cs:579-627` | `PowerHit`、`HitPlus`（`:586-590`：模式 3，目标非空支） | `PowerHit = false`、`nPower += HitPlus`、`canHit = true` |
| `GetHitMode(short) -> int` | `PlayObject.Attack.cs:80-138` | `PowerHit`（`:94-97`） | `RM_SPELL2` |
| `HitPlus` 字段 | `PlayObject.cs:189`；唯一写点 `RecalcHitSpeed` `:3704` | — | — |

**两个易错点**
1. `HitPlus` 是**上次 Recalc 的存量**，攻击路径**不会**因本击升级而重算 ⇒ `nPower += HitPlus` 用旧值。
2. `PowerHit` 的消费**先于** `AttackSuccess`（`GetAttackPowerHit` 在 `AttackDir` 的第二句），
   而 `canHit` 又决定 `AttackSuccess:250` 那次抽 ⇒ 三者是**同一击内的因果链**，不能拆到不同 tick。

### 1.3 `TwinHitSkill` 与 case 9（本项 ③）

| 符号 | 定义位置 | 读 | 写 / 发 |
| --- | --- | --- | --- |
| `TwinHitSkill` 字段 | `PlayObject.cs:185` | — | — |
| `AllowTwinHitSkill() -> bool` | `PlayObject.Base.cs:666-672` | — | `TwinHitSkill = true`、`LatestTwinHitTick = now`（**无冷却门**，与烈火不同） |
| `SkillTwinOnOff(bool)` | `PlayObject.Base.cs:626-639` | — | `TwinHitSkill = 开关` + 绿/红 sysmsg（`TwinHitOn/TwinHitOff`） |
| 施放入口 | `PlayObject.Attack.cs:1236-1252`（`case MagicConst.SKILL_TWINBLADE`=38） | `MagicArr[38] != null`、`MP >= GetSpellPoint` | `AllowTwinHitSkill()`、扣 MP、`SendSocket("+TWN")` |
| 超时 | `PlayObject.Message.cs:60-64` | `TwinHitSkill && now - LatestTwinHitTick > 60s` | `TwinHitSkill = false`、`SendSocket("+UTWN")` |
| 消费 | `PlayObject.Attack.cs:597-601` | `TwinHitSkill` | `TwinHitSkill = false`、`LatestTwinHitTick = now`、`nPower += Round(nPower/100*HitDouble*10)`、`canHit = true` |
| 帧判定 | `PlayObject.Attack.cs:124-128` | `TwinHitSkill` | `RM_TWINHIT`（否则保留 `RM_HIT`） |

**常量**：`SKILL_TWINBLADE = 38`（`src/Modules/SystemModule/Const/MagicConst.cs:162`）、
`SKILL_REDBANWOL = 56`（`:197`）、`SKILL_YEDO = 7`（`:32`）。

## 2. 现有 Rust 可复用面（先查再写，别重复造）

| 能力 | 现成实现 |
| --- | --- |
| `TrainSkill` / `CheckMagicLevelUp` / `CheckSkillProficiency` | `crates/world-bevy/src/magic.rs` 的 `train_skill` / `check_magic_level_up` / `check_skill_proficiency`（含 `SkillTrainState`/`MagicLevelUpOutcome`/`SkillProficiencyOutcome`），已由 `iter tests/spell_tail_vectors.rs` 与 `tests/parity/m3/spell-tail-probe`（真身反射）钉住 |
| `RM_MAGIC_LVEXP` 延迟投递与 SM 编码 | `crates/world-bevy/src/slave.rs`（`RM_MAGIC_LVEXP` 常量与投递载荷）+ `bins/game-svr/src/smframe.rs` 的 `SM_MAGIC_LVEXP(640)` 编码器 |
| 技能被动（`HitPlus`/`HitDouble`/`AttackSkill*`） | `bins/game-svr/src/skill_ability.rs`（IT-17b） |
| 动作帧映射与 `HitSkillState` | `bins/game-svr/src/attackwire.rs`（`rm_action_ident`/`HitSkillState`） |
| A 批的攻杀运行时与尾巴 | `World::set_attack_skill` / `drain_yedo` / `yedo_attack_tail_impl`（`crates/world-bevy/src/world.rs`） |

### 2.1 必须先解决的结构差异：`MagicArr` 是**拷贝**，不是引用

oracle 的 `MagicArr[MagIdx] = userMagic`（`PlayObject.cs:3684`）是**同对象引用** ⇒ 经 `MagicArr` 训练
升级会同时改 `MagicList`（存档同源）。本仓 IT-17b 的 `skill_ability::fill_magic_arr` 写的是
`arr[magic.mag_idx] = Some(m.clone())` ⇒ **`arr` 与 `list` 是两份**。

⇒ B 必须定一个权威面并写死：建议 **`sg.magic.list` 为权威**（存档、登录、训练都读写它），
`arr` 只作**派生快照**，在任何改动 `list` 的路径后**重填**（`fill_magic_arr` 只写不清，残影语义不变）。
A 的攻杀尾巴读的是注入的 `arr` 快照，因此训练后必须**在同一 tick 内重填**再跑尾巴，否则 `7-L` 用旧等级。

## 3. 帧面口径（§26）

| 行为 | 可见性 | 观测面 |
| --- | --- | --- |
| 熟练度/升级 | **帧面** | `SM_MAGIC_LVEXP(640)` → **仅施法者自己的 socket**；`Recog=MagicId`、`Param=Level`、`Tag=TranPoint`（lo/hi）；未升级 **3000ms**、升级 **800ms**（`UpdateDelayMsg` 会**覆盖**同 `(ident, MagicId)` 的旧条） |
| `PowerHit` 命中 | **帧面 + 数值面** | 模式 3 的动作帧 `RM_SPELL2 → SM_POWERHIT`（现 Rust 发 `SM_HIT`）；伤害数值 `+HitPlus` |
| `PowerHit` 置位 | 帧面（间接） | A 批的 `+PWR` 短包（已做） |
| `TwinHitSkill` | **帧面** | 施放技能 `+TWN`；到期 `+UTWN`；模式 9 动作帧 `SM_TWINHIT`（现 Rust 无条件发，是**反向**偏差） |
| `TwinHitSkill` 消费 | 帧面 + 数值面 | 模式 9 的 `SM_TWINHIT` 只在该标志为真时出现；伤害 `+Round(nPower/100*HitDouble*10)` |
| `TranPoint`/`Level`/`PowerHit`/`TwinHitSkill` 本身 | **状态面** | 只能由 640 的载荷、帧 identity 与伤害数值间接判 |
| `FastTrain` | 状态面 | 本仓未装载（`lib.rs` 四处已登记 `fast_train: false`）；`FastTrain=true` 只在「快速修炼项链」（`PlayObject.cs:2168`）出现，属装备面，另行登记 |

### 3.1 `Random(3)+1` 的可观测验法（RNG 消耗本身就是判据）

1. **纯缝计数**：对 `check_skill_proficiency`/`train_skill` 的调用点用 `CountingRng`（同 A 批用例①的做法）
   断言「恰 N 次、每次上界 3、值 = 同种子重放」。
2. **整击错位法**：沿用 A 批用例③的探针法——两个同种子世界，唯一差别是技能表；用**怪物探针**
   （`AttackPowerMode::Base`，每次挥砍恰 1 次采样）读出取数序列，断言两世界相差**恰好 k 次**
   （k = 该击的期望抽数）。这样"多抽/少抽"都会错位。
3. **站点级**（可选）：`MIR2_RNG_LOG` trace 的 `site` 列可直接看抽数落在哪个 `file:line`。

**前提断言（§43）**：每种验法都要先证明观测值随被测变量变化（探针值非常量、抽值序列非退化）。

## 4. 判据与用例草案

- **①训练**：`CheckSkillProficiency` 门内/门外两支（`Level<3`、`TrainLevel[Level] <= Abil.Level`）；
  `TrainSkill` 的 `FastTrain` ×3；`CheckMagicLevelUp` 的 `nLevel` 计算与「恰好扣到 0 才升」；
  640 帧的 **800ms/3000ms 两支**与 `UpdateDelayMsg` 的覆盖语义（同击同一 MagicId 只留一条）。
- **①时点**：同一击里升级后，A 的攻杀重置用**新** `Level`（`7-L`）；跨击不回退。
- **②`PowerHit`**：模式 3 且 arm ⇒ `SM_POWERHIT` + 伤害恰多 `HitPlus` + 标志清零（第二击不再加成）；
  未 arm ⇒ `SM_HIT` + 无加成；`canHit` ⇒ `AttackSuccess:250` 的攻杀抽**恰多 1 次**。
- **③`TwinHitSkill`**：施放 38 后 `+TWN`；模式 9 帧为 `SM_TWINHIT` 且伤害按 `HitDouble` 加成；
  未施放时模式 9 发 `SM_HIT`（**修掉现 Rust 的无条件发**）；60s 到期 `+UTWN` 且恢复 `SM_HIT`。

## 5. 牙（草案）

| 变异 | 应红 |
| --- | --- |
| 删 `Random(3)+1` 抽（三个调用点各一颗） | ① 的计数用例 |
| `CheckSkillProficiency` 门从 `Level<3` 改成 `Level<=3` | ①门用例 |
| `TrainSkill` 去掉 `FastTrain` ×3 | ①训练量用例 |
| `CheckMagicLevelUp` 把 `nLevel` 恒 0 | ①升级用例 |
| 640 帧延迟 800↔3000 互换 | ①帧时序用例 |
| `PowerHit` 消费不清零 | ②第二击用例 |
| `nPower += HitPlus` 删掉 | ②伤害数值用例 |
| `rm_action_ident` case 9 改回无条件 | ③帧用例 |
| `TwinHitSkill` 超时删掉 | ③到期用例 |

## 6. 决策点（需审阅裁定，未定不写码）

- **D-1 `TrainCurrentSkill`（`PlayObject.Attack.cs:337`）是否并入 B？**
  它是**每次命中都跑**的第二个训练入口（同一套三个函数，门不同：`Level < TrainLv` 而非 `< 3`），
  自带 `Random(3)+1` 若干次。
  - **并入**：账本完整；代价是多一张「命中模式 → 技能」门表（3/4/5/7/8/9/12 各一条）。
  - **不并入**：B 的抽数账本只覆盖 `AttackSuccess:241-260`，`TrainCurrentSkill` 恒 0 抽——这是**结构性 0**，
    必须像 A 批那样用"错位法"钉住并把"未覆盖"写死；否则任何带技能角色的实战取数都会整体错位。
  - **我的建议：并入 B（作为第二步）**。理由是同一批函数的两个调用点，只接一个就是"只搬一半"。
- **D-2 `FireHitSkill`（模式 7）是否并入？**
  它与 `TwinHitSkill` 同族（`AllowFireHitSkill` `PlayObject.Base.cs:653-664`、20s 超时 `PlayObject.Message.cs:54-59`、
  消费 `PlayObject.Attack.cs:591-596`），但**不在**审阅给的三项字面范围。若不并入，模式 7 的
  `SM_FIREHIT` 帧与加成仍是缺口（现 Rust `rm_action_ident` 的 `7 if skill.fire_hit_skill` 分支恒假）。
  **建议：单列 B3**，理由同 C 的拆法（帧面/状态面不同）。
- **D-3 同 tick 双击的帧判定**：动作帧在宿主即时发（`handle_hit_cmd`），`PowerHit` 消费在下一 tick。
  同一 tick 内两击时 oracle 是「第一击消费、第二击不再加成」，本仓两帧都会按"消费前"的标志发 ⇒
  **帧面偏差**。要么接受并登记（建议），要么把动作帧推迟到 tick（大改，不建议在 B 做）。

## 7. 工作量（粗估）

- ①②③ 主线（含复用现有 `magic.rs`、`MagicArr` 权威面重填、`attack_with_grouped` 增加
  `GetAttackPowerHit` 等价参数与 `canHit` 返回）：**1.5–2 天**（含用例与牙）。
- 若并入 D-1（`TrainCurrentSkill`）：**+0.5 天**。
- D-2（`FireHitSkill`）若并入：**+0.5 天**。

## 8. 边界复述

不改 657（`2f` 在飞）；不做 C；不改 `ClientHitXY` 的 `wHitMode` 分派与既有动作帧序（除 case 9 的
条件化）；`FastTrain` 保持现状（未装载，登记）。
