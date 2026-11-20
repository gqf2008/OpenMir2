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

## 6. 第一批（结构层）落地 — 2026-11-20

**范围**：`RecalcHitSpeed` 的技能被动 + `MagicArr` 填充 + 752 帧面；**不含**攻杀剑法的 RNG 抽数（留后续批）
与战斗消费（`GetHitMode`/`SkillAttackDamage`）。

**实现（`bins/game-svr/src/skill_ability.rs`）**：
- `SkillPassives{ hit_point, hit_plus, hit_double, attack_skill_count }` 与
  `recalc_hit_speed_skills(magics, arr: &mut [Option<MagicRcd>])`：四条技能公式（基本剑法 3L / 精神力
  `Round(8/3L)` / 攻杀 `L`+`HitPlus=5+L`+`Count=7-L` / 烈火 `4+4L`）、`MagicArr` **只写不清**、
  `MagIdx>=50` **跳过并 WRN**（oracle 越界崩溃；登记的原样差异）。
- `ability::recalc_abilitys_with_prior_and_skills(…, skill)`：`HitPoint = hit_base + skill.hit_point + AddAbil.HIT`
  （顺序同 oracle：技能 → 装备）；`enter::recalc_for_session_with_skills` 给登录用。
- `SessionGround.magic` 增 `arr: Vec<Option<MagicRcd>>(50)` + `passives`；写点：登录
  `session_ground_from_login`（由记录 `magic` 过滤结果算）+ 每次 `recalc_session_ability`。**只写不清**。
- **752 字段名订正（重要）**：oracle `PlayObject.Message.cs:1675` 的实参是
  `MakeMessage(SM_SUBABILITY, /*Recog*/MakeLong(MakeWord(AntiMagic,0),0), /*Param*/MakeWord(HitPoint,SpeedPoint), /*Tag*/MakeWord(AntiPoison,PoisonRecover))`
  ⇒ 线上 **`Param` = HitPoint(lo)/SpeedPoint(hi)**；`Tag` 是 AntiPoison/PoisonRecover（此前口述的
  "Tag=MakeWord(HitPoint,SpeedPoint)" 是 oracle 形参名，不是线缆字段名）。

**测试/牙**（`cargo test -p game-svr --lib skill_ability`，6 颗）：
- 公式四条 + Level 门 + Round 半数 + `MagicArr` 不清空 + `MagIdx>=50` 忽略；
- 会话级：`recalc_session_ability` 把技能被动带进 `sub.hit_point` 与 **752 `Param`**；删技能后
  `MagicArr` 仍非空；
- 牙：M1 `9/3→8/3`（公式红，left 18/right 19）；M2 函数开头 `arr.fill(None)`（不清空断言红）；
  M3 `recalc_session_ability` 回 `with_prior`（752 无技能：left 5/right 14）。还原 sha 相等 + 基线复绿。
- 基线（本批跑绿）：`it08_`(3) / `recalc_`(16) / `hit_mode_conditions`(1) / `it17a_`(6) / `sm_addmagic`(2)；
  `clippy -p game-svr --all-targets -D warnings` 绿。

**已登记的 known deviation（有意不同，别当 bug 修）**：
- **`MagIdx >= 50`**：oracle `MagicArr[userMagic.MagIdx]`（数组 50，`PlayObject.cs:1041/3684`）会抛
  `IndexOutOfRangeException`（进程级崩溃）；本仓 `recalc_hit_speed_skills` **跳过该条目并 WRN**
  （理由：不把内容数据的错误升级成进程崩溃）。保留用例 `magic_arr_ignores_id_out_of_range`
  （断言越界条目不写、合法条目照算）——删掉「跳过」或改成写入都会红。
- **本批 0 抽是结构性的**：`recalc_hit_speed_skills` **没有 `rng` 形参**，攻杀支只置 `AttackSkillCount`
  不置 `AttackSkillPointCount`；只要 `MagicList` 不含攻杀剑法就没有任何抽数。**第二批加 `rng` 后
  本句不再成立**（届时「带攻杀 ⇒ 每次 Recalc 恰 1 抽」要进账本与断言）。

**后续批（未做）**：
- **攻杀的 RNG 两处**：① `RecalcHitSpeed` 的 `PlayObject.cs:3706`（每次 Recalc 恰 1 次，**Level=0 也抽**）；
  ② **攻击时第二抽** `PlayObject.Attack.cs:943-952`——有攻杀且武器 `Dura>0` 时每击 `AttackSkillCount -= 1`，
  归零则 `Count = 7-L` 并 `AttackSkillPointCount = RandomByte(7-L)`（**再抽 1 次**）同时
  `PowerHit = true; SendSocket("+PWR")`。**别以为 RNG 只有 Recalc 一处**。
- `SkillAttackDamage` 的次级伤害（模式 4/5/8/12 的 `nSecPwr`）；
- `HitPlus/HitDouble` 的战斗态；`PowerHit/FireHitSkill`（`GetHitMode` 模式 3/7）——与上面的 `+PWR` 同批。

## 7.1 3a 返工（Level 门位置纠正）— 2026-11-20

**偏差**：oracle `PlayObject.cs:3699-3707` 的 `case SKILL_YEDO` 里**只有 `HitPoint += L` 在
`if (Level > 0)` 内**；`HitPlus = DEFHIT + L`、`AttackSkillCount = 7 - L`（以及 `RandomByte(7-L)`）
在门**外**（`:3704-3706`）⇒ Level=0 也置 `HitPlus=5`/`Count=7`/抽 1 次。第一批把四件事整体放进
门内，且单测 `level_zero_skills_add_nothing_but_fire_sword` 把错期望（`hit_plus==0/Count==0`）钉住了。

**修法**：`HitPlus/AttackSkillCount` 移出门（`HitPoint` 保留门内）；单测期望改为
`HitPoint=0 / HitPlus=DEFHIT / Count=7 / HitDouble=4`，注释写明"旧期望错在哪"。
**牙**：把两者再塞回门内 ⇒ 该用例红（还原 sha 相等后复绿）。
**快照 diff（3a 后）**：`E:/UserTemp/it17b3a-run.txt`（`2100 passed / 0 failed / 65 ignored`）对基线
⇒ green→red = **空**；new = 第二批三条（`hit_skill_state_reads_magic_arr_only` + 两条 hit_mode）；
missing = 无。

## 7. 第二批（`GetHitMode` 的 `MagicArr` 三条件接线）— 2026-11-20

**范围声明（防日后默认扩大）**：本批**只接 `MagicArr` 的 `ergum(12)/banwol(25)/crossmoon(34)`**
三个条件（`PlayObject.Attack.cs:101-134` 的 `case 4/5/8`）；`PowerHit`/`FireHitSkill`（`case 3/7`
的战斗态）**不在本批**，`hit_skill_state` 恒传 `false`。

**接线**：`lib.rs` 的动作帧分支（原 `HitSkillState::default()`）改为
`skill_ability::hit_skill_state(&sg.magic.arr)`（`sessions.get(chr_name)`；缺会话回默认）；
`skill_ability` 增 `SKILL_ERGUM/SKILL_BANWOL/SKILL_CROSSMOON` 常量（`readbook.rs` 改为 `pub use`，
避免两处常量漂移）。

**接线前基线快照（路径 + 命令 + 时间；`3b` 复核用）**：
- `E:/UserTemp/it17b2-baseline-list.txt`（`cd mir2-rs && CARGO_BUILD_JOBS=2 cargo test -p game-svr --lib -- --list`，2162 名）
- `E:/UserTemp/it17b2-baseline-run.txt`（`… cargo test -p game-svr --lib`；`2097 passed / 0 failed / 65 ignored`，逐条 ok）
- `E:/UserTemp/it17b2-attack-subset.txt`（名单中含 `attack|hit|struck` 的 103 条）
- `E:/UserTemp/it17b2-baseline-meta.txt`（时间 `2026-11-20T11:53:36+08:00`、head `1bfc59b28`、master `00cc9e761`、命令原文）
- 接线后复跑：`E:/UserTemp/it17b2-after-wiring-run.txt`（`2100 passed / 0 failed / 65 ignored`）⇒
  **逐名 diff：green→red = 空**（"没变"也是结论）；新增 3 条（下），无消失。

**新用例（正反各一）**：
- `it17b_hit_mode_skill_conditions_positive`：会话 `MagicArr` 有 12/25/34 ⇒ `CM_LONGHIT/CM_WIDEHIT/CM_CRSHIT`
  的动作帧 = `SM_LONGHIT(19)/SM_WIDEHIT(24)/SM_CRSHIT(25)`。
- `it17b_hit_mode_skill_conditions_negative`：`MagicArr` 全空 ⇒ 同三条命令仍 `SM_HIT`（且非三种特技帧）。
- `skill_ability::tests::hit_skill_state_reads_magic_arr_only`：只认 12/25/34，`PowerHit/FireHitSkill` 恒 false。

**牙（红→还原 sha→复绿）**：M4/M5/M6 分别把 `ergum/banwol/crossmoon` 置 false ⇒ 正向例红；
M7b 三条件恒真 ⇒ 反向例红。还原后基线复绿。

## 8. 第三批（3b：攻杀 `RandomByte(7-L)` 的 RNG 账本）— 2026-11-20

**两步走（按 opc-19 裁定）**：
- **3b-1 签名步**（语义不变）：`recalc_hit_speed_skills(..., rng: Option<&mut dyn RandomSource>)`、
  `recalc_session_ability(..., rng)`；**28 个调用点全传 `None`**；登录装配拆出 `fill_magic_arr`
  （只补填 `MagicArr`，**不重算**被动——否则攻杀会在登录时二次抽数）。全量 `--lib` = **2100/0/65**
  与 3a 完全同名单（三行口径：green→red / new / missing 均为空）。
- **3b-2 语义步**：生产 **15 个** `recalc_session_ability` 调用点改传 `Some(&mut world.rng)`；登录
  `login_skill` 也传 `Some(&mut world.rng)`；`drain_buff_downlink` 的 `world: &World` 升为
  `&mut World`（8 个调用点；两个测试站点先 `world.last_tick.clone()` 再传，避免借用冲突）。
  `case SKILL_YEDO` 抽 `RandomByte(7-L)`（**L=0 也抽**，`PlayObject.cs:3706` 在 Level 门外），
  结果进 `SkillPassives.attack_skill_point_count`。

**用例（记录器自证 + 固定种子双钉）**：
- `yedo_recalc_draws_once_per_recalc`：L=2（`7-L=5`）两次 Recalc ⇒ 记录器 `random_below` **2 次**，
  值 == 同种子独立重放的两次值；`attack_skill_point_count` == 第二次值。
- `yedo_level_zero_still_draws_once`：L=0 ⇒ **1 抽**，`Count=7`，抽值 == 记录器。
- `no_yedo_draws_zero`：只含基本/精神/烈火 ⇒ 记录器 **0 抽**。
- 牙：M1 删抽 ⇒ 用例①红；M2 循环外无条件抽 ⇒ 用例③红；还原 sha 相等、复绿。

**结果**：全量 `--lib`（12:19）= **2103 passed / 0 failed / 65 ignored**；对 3b-1 逐名 diff：
green→red **空**、new = 上述 3 条、missing **无**。

**仍不做（卡 §6 已列）**：**攻击时第二抽** `PlayObject.Attack.cs:943-952`（`AttackSkillCount` 归零
重抽 + `PowerHit=true; SendSocket("+PWR")`）与 `PowerHit/FireHitSkill` 战斗态——属攻击批。
