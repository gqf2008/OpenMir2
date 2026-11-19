# openmir2-caster-reachability

> 状态：**登记卡（2026-11-20，opc-07 发布）**。触发：M59（魔法 16 条金标准登录序重生成）签署时 QA 指出的可达性边界。
> 关联：`docs/issues/openmir2-golden-fixture-drift.md` §8.9f、`docs/issues/openmir2-magic-parity.md`

## 1. 现状（事实）

M59 之后，魔法 16 条 verified 的依据是**登录序重生成件**（`Fixture.anti_magic=1`、`MaxHP`=等级公式、`construction` 机检）。但 QA 在签署中指出：

- **目标侧**：L20 战士、`MaxHP=224`、`AntiMagic=1`、`HP ≤ MaxHP` —— 均为登录序可达，且被契约机检。
- **施法者侧**：仍是**合成输入** —— `CasterLevel=30`、`MC=15`、`SC=15`，**不在 `construction`/`derived` 里、也没有登录派生复算**。
- 更进一步：**战士施放火球/雷电这类法术本身不是生产可达组合**（法师技能由法师施放）。

⇒ 当前 verified 证的是"**目标状态可达时引擎等价**"，**不是**"整条施法链（施法者属性 → 施法 → 受击）生产可达"。

## 2. 目标

把施法者侧也纳入登录序可达与机检：

1. `construction` 增 `caster: {job, level, equipment[], skills[]}`，`derived` 增施法者侧字段（`mc/sc/level/max_hp/max_mp`）；
2. 生成器对施法者同样走 `RecalcLevelAbilitys()+RecalcAbilitys()` 复算，与 `Fixture` 声明的施法者值断言相等，不等 `rc=2`；
3. **法术与职业匹配**：每种法术由**该法术所属职业**（法师/道士/战士）的可达角色施放；跨职业施法若保留，须显式标为"非生产可达场景"并不得作 verified 唯一依据；
4. 比较器增"施法者侧 consistency"检查（复用 `check_golden_contract`）。

## 3. 验收

- 16 条魔法中，每条给出"施法者 = 哪个职业、哪个等级、哪套装备/技能"的构造路径，且**机检可复算**；
- 重生成后与现有件的行为差异逐条列出并解释（施法者属性变化会改伤害/命中/MP 消耗，**帧面变化是预期内**）；
- 若某条因职业/等级组合不可达而无法构造，如实登记为 `unreachable` 并从 verified 依据中剔除（覆盖率可能下降，代价接受）。

## 4. 不做

- 不改比较器判据以迁就旧件；
- 不把装备/技能加成引入 `derived`（维持 M59 的 fail-closed），除非另卡评审。
