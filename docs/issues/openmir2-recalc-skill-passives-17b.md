# openmir2-recalc-skill-passives-17b

> 状态：**待认领（2026-11-20 立卡，来源 = IT-17a ReadBook 交付时切出的能力层缺口）**。
> 本卡是 items 线 **IT-17b**：`PlayObject.RecalcAbilitys` 的**技能被动**建模。IT-17a（读书/学技能）
> 已交付学习面（见 `docs/issues/openmir2-items-parity.md` §9.15），**本卡只补被动的重算与战斗效果**。

## 0. 现状（代码事实）

- oracle `RecalcAbilitys`（`src/M2Server/Player/PlayObject.cs`）在 `:3681-3684` 用 `MagicList` 填
  `MagicArr[MagIdx]`，并按技能等级改 `HitPoint/SpeedPoint/DC/MC/SC…`（`SKILL_ONESWORD` 基本剑法、
  `SKILL_ILKWANG` 精神力战法等分支）；Rust `ability::recalc_abilitys` **未读技能层**。
- `mir2-rs/bins/game-svr/src/attackwire.rs` 头注释明确登记：`MagicArr[SKILL_ERGUM|BANWOL|CROSSMOON]`
  条件**恒假**（技能层未移植）⇒ 普攻的 `GetHitMode` 模式 3/4/5/7/8 分支不可达。
- IT-17a 的写点已就位：`SessionGround.magic.list`（`PlayObject.MagicList` 快照，登录 + 学成维护，
  `build_save_record` 回写）；657 帧目前按**同值**发（`GetCharStatus()` + 当前 `HitSpeed`）。

## 1. 缺口（行为差异的具体面）

1. **属性被动**：学会「基本剑法/精神力战法/…」后 oracle 的 `HitPoint/SpeedPoint`（进而命中/攻速）变化，
   Rust 不变 ⇒ `SM_ABILITY(52)` / 657 / 后续攻击判定都会偏。
2. **`MagicArr` 派生**：oracle 的 `MagicArr[id] != null` 是**技能层就位**的判据（普攻 `GetHitMode`、
   `ThrustingOnOff`/`HalfMoonOnOff` 的后续开关）；Rust 无此表 ⇒ 战斗模式分支不可达。
3. **`WAbil` 重算**：`RecalcAbilitys` 的 `WAbil = Abil.Clone()` + 逐项叠加（含技能项）在 `recalc` 路径
   的落点与 oracle 的触发时机（登录/升级/换装/学技能）需逐一对齐。

## 2. 可观测面（§26）

- `SM_ABILITY(52)` 尾（`TAbility`/`HitPoint`/`SpeedPoint` 等）逐字段；
- `SM_CHARSTATUSCHANGED(657)`（`status/hit_speed`）；
- 普攻 `GetHitMode` 的分支（`attackwire` 已把条件表列出：`MagicArr[12/25/…]`）；
- 学技能后下一次攻击的 `SM_HIT`/`SM_STRUCK` 参数。

## 3. 验收（DoD 草案）

- `recalc_abilitys` 读 `magic_list` 并复刻 `PlayObject.cs:3681-` 的技能分支（逐分支测试）；
- `MagicArr` 等价物（`HashSet<u16>` 或 `Vec<Option<..>>`）从 `MagicList` 派生，`attackwire` 的
  `MagicArr` 条件接上并逐模式对拍（含 `SKILL_ERGUM=12` / `SKILL_BANWOL=25` / 逐日剑法）；
- IT-17a 的 readbook 对拍把 657 从「同值」升级为「oracle 重算值」；`it17a` 卡改 `verified`；
- 牙：删每个技能分支 ⇒ 对应属性/模式用例红；`MagicArr` 不派生 ⇒ `GetHitMode` 模式用例红。

## 4. 边界

- 不改 `src/` C# 参照实现；不动 readbook 的学习四门与帧序（17a 已对拍）。
- 技能**熟练度/升级**（`RM_MAGIC_LVEXP`/`TrainSkill`）不属本卡，除非验收明确纳入。

## 5. oracle 事实补录（2026-11-20，`opc-19` 派 read-only 核对）

### 5.1 技能被动写哪些字段（`PlayObject.RecalcHitSpeed`，`PlayObject.cs:3659-3713`，由 `BaseObject.Abil.cs:54` 早于装备循环调用）
- 职业底：战/法 `HitPoint = DEFHIT(5) + BonusAbil.Hit / BonusAbilof*.Hit`、`SpeedPoint = DEFSPEED(15) + BonusAbil.Speed / *`（`:3666-3675`）；**道士只赋 `SpeedPoint = 15 + Speed/30 + 3`，`HitPoint` 不赋值**（`:3676-3679`）⇒ 道士 `HitPoint` 跨 Recalc 累积（`BaseObject.Abil.cs:65` 与 `PlayObject.cs:2891` 各加一次 `AddAbil.HIT`，无重置）。
- 技能项（`MagicList` 顺序，`:3681-3712`）：基本剑法 `HitPoint += Round(9/3*L)`；精神力 `+Round(8/3*L)`；攻杀 `+L`+`HitPlus = DEFHIT+L`+`AttackSkillCount = 7-L`+**`AttackSkillPointCount = RandomByte(7-L)`（每次 Recalc 抽 1 次 RNG）**；烈火 `HitDouble = 4+4L`。
- 装备 `AddAbil.HIT/SPEED` 在**技能之后**叠加（`:2890-2891`，`(byte)` 截断）。

### 5.2 `MagicArr`（`PlayObject.cs:3684`）
- 唯一写点；数组 `new UserMagic[50]`（`:1041`）⇒ **魔法 id 必须 < 50**（≥50 会在 `RecalcHitSpeed` 抛 `IndexOutOfRange`，实测 61）。**从不清空** ⇒ 删技能后留残影（`SendDelMagic` 只发帧）。
- 下游：`GetAttackMagic`(`:5401`)、`GetHitMode` 模式 4/5/8/12（`src/M2Server/Player/PlayObject.Attack.cs:101-134`，`RM_LONGHIT/WIDEHIT/CRSHIT`）、`SkillAttackDamage`(`src/M2Server/Player/PlayObject.Attack.cs:142-192`)、熟练度/升级(`:241/249/360-378`)、登录自动开关(`PlayObject.cs:1425`)。

### 5.3 **ExtraAbil 在 oracle 里不会进入最终 `WAbil`**（重要）
- `ExtraAbil[...]` 的**读点只有一处**：`PlayObject.RecalcAbilitys` 的 `:2818-2846` 块——它对 `WAbil.DC/MC/SC/MaxHP/MaxMP/HitSpeed` 做加/乘。
- 但同函数**随后**在 `:2878 RecalcAdjusBonus()`、`:2890-2891 SpeedPoint/HitPoint += AddAbil`、`:2899 HitSpeed = AddAbil.HitSpeed`、`:2900-2906 WAbil.MaxHP/MP/AC/MAC/DC/MC/SC = Abil + AddAbil（逐字节 _MIN(255,和)）`——**把 ExtraAbil 块写下的值整体覆盖**（`HitSpeed` 也被覆盖）。
- 全仓检索：`ExtraAbil[...]` 的其它出现仅为**写点**（`PlayObject.Base.cs:2105-2147` 吃 Shape12 时置 `_MAX`）与**到期清零**（`PlayObject.Message.cs:165-169`）；`BaseObject.Abil.cs:155` 是一行**注释掉的**同类读；`GetAttackPower`（`PlayObject.cs:2910-2950`）不读 `ExtraAbil`。⇒ **oracle 的 ExtraAbil 统计效果是死路**：不进 `WAbil`、不进 `SM_ABILITY(52)`、不进 657 的 `HitSpeed`、也不进 `GetAttackPower`。
- 由此对 **IT-07** 的判据推论（已转 `43` 核实现）：吃 Shape12 后的正确判据是 **`WAbil` 不变**；唯一可观测副作用是那 1-6 条蓝色 `SysMsg`（`RM_SYSMESSAGE`）+ `RM_ABILITY` 排队泵出的 52（WAbil 仍为未加 buff 的值）+ 状态数组 `ExtraAbil/Flag/Times`（仅状态面）。若 IT-07 的实现把 buff 加进 `WAbil`，它**比 oracle 多做了一步**。
- 同机制旁注：`RecalcAdjusBonus` 写在 `:2878` 的 `WAbil`/`MaxHP/MP` 变化也在 `:2900-2906` 被覆盖（BonusAbil 的 WAbil 面同样不持久；其消费在别处）。

### 5.4 可观测面（§26）
- `SM_ABILITY(52)` 尾 = `EDCode(EncodeMessage(WAbil))`（`PlayObject.Message.cs:1733-1734`）——**无 HitPoint/SpeedPoint**。
- **`SM_SUBABILITY(752)`**：`Tag = MakeWord(HitPoint, SpeedPoint)`（`PlayObject.Message.cs:1675/1935`）⇒ 技能被动的唯一帧面；登录簇里也有一条 752。
- `657` 只有 `HitSpeed`（不含 HitPoint/SpeedPoint）；`HitPlus/HitDouble/AttackSkillCount/AttackSkillPointCount` 无帧，仅状态面；模式/次级伤害走攻击帧（R 级）。

### 5.5 开工前置基线
- `--lib it07_` 在**IT-07 合入 master 之后**才有效（当前树零命中——IT-07 只在它的分支上）。在此之前用：
  `cargo test -p game-svr --lib it08_`（Recalc↔657/WAbil）、`--lib recalc_`（含 `recalc_reports_*`/`bag_takeoff_recalc_uses_prior_subability`）、
  `--lib hit_mode_conditions_fall_back_to_rm_hit`（attackwire）、`--lib it17a_`（学技能入口）、`--lib sm_addmagic`。
- 只要角色带攻杀剑法，**每次 Recalc 抽 1 次**（`:3706`）⇒ 夹具固定 `MIR2_RNG_SEED` 并把该抽记进账本，或选无攻杀的角色做纯被动用例。
- 动 `MagicArr` 派生时，再加 `attackwire` 的模式用例（`MagicArr` 条件直接决定 `RM_LONGHIT/WIDEHIT/CRSHIT`）。
