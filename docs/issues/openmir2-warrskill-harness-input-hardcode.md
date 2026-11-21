# warrskill 对拍装置：输入字段硬编码，改 golden 输入 Rust 不会跟随（只立卡，未实现）

> 状态：**已知项登记**（2026-11-21）。只立卡、不实现；修它会动共享 harness、影响 10 份 golden。

## 1. 现状（硬编码输入 = 死数据，不是覆盖面）

`mir2-rs/bins/game-svr/src/warrskill_parity_tests.rs` 的 `run_case` 里这些输入**不读 golden**，
而是写死：

| 输入 | golden 字段 | 装置实际值 |
| --- | --- | --- |
| Caster 等级 | `Fixture.caster_level` | 硬编码 L30（`state.wabil.hp/max_hp = 419` 按 L30 公式） |
| Caster MC/SC | `Fixture.caster_mc/caster_sc` | 不设（默认） |
| Target 等级/最大血 | `Fixture.target_level/target_max_hp` | 默认 |
| AntiMagic | `Fixture.anti_magic` | 默认 |
| 魔表 Spell/DefSpell | `Fixture.magic_spell/magic_def_spell` | 硬编码 `spell:20, def_spell:0` |
| Seed | `Fixture.seed` | **未安装 RNG 种子**（`cover_drain` 下当前用例零抽，故未暴露） |

⇒ **golden 改这些输入，Rust 不会跟随**：两侧起点不同仍可能全绿（"输入漂移无牙"）。
与 `mbo43_kill` 预设 bug 同族、方向相反：那是"golden 记了状态、装置没按它跑"；这是"golden 记了输入、装置根本没读"。

## 2. 影响面

（装置对照的 oracle 入口：`src/M2Server/Player/PlayObject.Attack.cs:1119-1275` 的 `ClientSpellXY`。）

- 受影响：`warr_3/4/7/12/25/26/27/34/38/43` 共 **10 份 golden**（`warrskill_parity_tests` 全部用例）。
- 其中 26/38 的 MP 门、27 的坐标方向、53 的 `Param/Series` 依赖这些输入值 ⇒ 输入漂移可能改变期望而不红。

## 3. 修法（候选，未实现）

1. `run_case` 从 `Fixture` 读 `caster_level/caster_mc/caster_sc/target_level/target_max_hp/anti_magic/magic_spell/magic_def_spell/seed`；
   - 等级派生沿用 `recalc_session_ability`（避免继续写死 HP=419）；
   - `seed` 通过 `world.rng = RandomNumber::with_seed(...)`（是否允许测试改种子需评估）。
2. 同时把 golden 的 `fixture_spec`/`Fixture` 重复字段收敛成单一来源（当前两处都有同值字段）。
3. **回归口径**：10 份 golden 全部复跑；若某份因"终于按真实输入跑"而红，说明它此前靠死数据通过 ⇒ 逐条定性，不得调期望洗绿。

## 4. 关联

- 卡 `docs/issues/magic-52-clientxy-explicit-case.md` §3b（43 的行为面断言）；
- 口径："夹具硬编码的输入字段不是覆盖面，是死数据"（全组台账）。
