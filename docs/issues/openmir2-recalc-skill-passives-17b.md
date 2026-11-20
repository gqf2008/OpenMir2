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
