# openmir2-golden-fixture-drift

> 状态：**事故登记 + 处置记录（2026-11-20，opc-07 发布）**。触发：`a2` 按"元数据类/行为类"口径重生成入库魔法金标准时，发现 **6/14 行为类差异非空**。
> 关联：`docs/issues/openmir2-magic-parity.md`、`docs/qa-charter.md` §18 / §18.1、`docs/issues/openmir2-warrskill-entry.md`

## 1. 现象

用现行装置（`MagicGolden`，master 2026-11-20）重生成入库的 14 条魔法 golden，与入库件逐接收者逐字段比较：

- **行为类差异非空 6 条**：`lightning-11`、`lightning-45`、`fireball-5`、`firecharm`、`hbfireball`、`healing`
- 实例（`lightning-11`）：入库 target 仅 2 条 RM、无伤害（HP 100）；重生成 6 RM / 5 SM（多 `SM_STRUCK(31)`、`SM 656`）、HP 85；`Rng` `[2]` vs `[2,0,0,0,6]`；53/31 帧 `MaxHP` 100→224；638 尾编码文本不同。
- 证据：`E:/UserFleet/qa/warrskill/golden-classification-20261120.txt`（259 行，逐字段 old/new 原文）、重生成件 `E:/UserFleet/qa/warrskill/regen/`。

## 2. 定位（符号锚）

1. **漂移集合 = 构造器集合 × golden 世代**：`Program.cs` 的 `--case` 分派表里，漂的 6 条恰好 = `AntiMagicBatch`（lightning-11/45、firecharm、hbfireball、fireball-5）+ `HealingBatch`（healing）；不漂的 = `FireballCase`、`BehaviorBatch`、`ShowHpBatch`、`BigHealingBatch`。
2. **重算的引入**：`git log -S'RecalcLevelAbilitys'` / `-S'RecalcAbilitys'`（限 `MagicGolden/Program.cs`）恰好命中 `6e86a0c10`（14/15/31 按登录序重生成）与 `fadb5db5e`；这 6 条 golden 的最后修改提交为 `e1c2a7952`/`3f2b8c632`/`73ba13013`/`5251b9d54`/`bf7aa4271`，**均早于重算引入**；而 `fireball`(`c5a6c4484`)、`shield`(`e46644102`) 同样早于却**不漂**（它们不消费被重算的字段）。
3. **装置现行顺序**（`RunMagicCase`）：ctor 手设 `AntiMagic=3` → `RecalcLevelAbilitys()` → `WAbil.HP=100` → `RecalcAbilitys()`。`BaseObject.Abil.cs` 的 `RecalcAbilitys()` **无条件** `AntiMagic = 1;` 再 `+= AddAbil.AntiMagic` ⇒ **现行装置已产不出 `AntiMagic=3`**。
4. **oracle 登录路径确实调重算**：`WorldServer.cs` 的 `playObject.UserLogon();` → `PlayObject.cs` 的 `public void UserLogon()` 内顺序调 `RecalcLevelAbilitys(); RecalcAbilitys();`（`BoReadyRun` 守卫，一次性）。
5. **Rust 侧夹具当前是 `anti_magic=3`/`max_hp=100`**（23/28 另设 224）⇒ **两侧本同值，对拍结论本身没错**。

## 3. 已裁定（并已自我更正一次）

- **不是"金标准错"，也不是"Rust 错"**：两侧对同一输入状态一致；问题是**装置换代后欠一次重生成**，以及**该输入状态的生产可达性未验**。
- **撤回**："6 条应降级、覆盖率 42→36"——判早了。`MaxHP=100` **是** 11 级战士的登录派生值（`RecalcLevelAbilitys` 战士支 `14 + round(((L/4) + WarrHPRate + L/20) * L)`，部署 `LevelValueOfWarrHP=4`/`WarrHPRate=4.5`；L=11→100、L=20→224，后者与装置观测值吻合）。Rust 侧 `crates/formula` 端口独立给出同样的 100/224。**"装置产不出" ≠ "生产不可达"**（章程 §18.1）。
- **现状**：6 条 + `MG-10`（`fireball`，同源旧夹具）**状态不动**，覆盖率保持 42。

## 4. 未决（本卡的开放项）

1. **`AntiMagic=3` 的生产可达性 = UNVERIFIED**（`2d` 只读清点，`E:/UserFleet/qa/warrskill/mgavoid-reachability-20261120.md`，82 行）：
   - 冻结 `sql/mir2_data.sql`（sha `9b62084a…`，`CREATE TABLE stditems`）里 `Mgavoid>0` 的物品 **0 行**；
   - BugSet(+1) 的 Shape 163/164/165 只命中两个 StdMode 31 消耗包（非首饰）；
   - 祖玛/龙套(+6) 现存件 `NeedLevel=47`，L11 不可穿、且缺 7 件；
   - 实例随机属性（`Desc[7]/[11]/[12]`，`GameItemSystem.cs`）属掉落生成，冻结数据不覆盖 ⇒ **判定不足**。
   结论：**不许**据此降级，也**不许**当"已可达"；需要"真实实例夹具"或活库样本才能定。
2. **装置参数化（先报设计后实现）**：把"是否走登录序"与"对象等级/`AntiMagic`/`MaxHP`"变成 golden `Fixture` 的**显式字段**，默认值与旧件标注（`unreachable synthetic` / 世代标注）一并设计。
3. **补一条生产可达的补充场景**：`AntiMagic=1`（无装备、登录后形态）+ 登录序等级，作为这 6 条魔法的**主场景**；旧"合成角色"场景降为**附加用例**并标注。
4. **元组自洽性**：旧 `fixture_spec` 同时写 `target_level=20` 与 `target_max_hp=100`，而 L20 的派生值战 224/道 131/法 77 ⇒ "L20 且 MaxHP=100"不是任何 Job 的登录派生状态。审计须分两层：**被消费字段的可达性**（魔法路径实际读哪些目标字段，给符号锚）与**元组自洽性**（自相矛盾组合须在卡面标注）。
5. **可达性审计推广**：对**全部 `verified` 条目**（四域）逐条判"可达 / 不可达 / 不适用"，不可达者列出。`3b` 先提范围，我批。

## 5. 验收

- 参数化设计经 opc-07 批准后实施；重生成前后逐字段比对，**行为类差异必须为空或逐条解释**；
- 6 条 + `MG-10` 的最终状态由 opc-07 依据"可达性结论 + 新场景证据"决定；
- 任何 status 变更单独一笔、走分支、经 QA 签署，**禁止与代码/装置改动混提**。
