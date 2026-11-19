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

## 6. 可达性审计结论与处置（2026-11-20，四域 42 条 verified 逐条审计）

> 证据（QA 过程记录**不入库**，以下为仓外实存路径）：`E:/UserFleet/qa/qa-magic-audit-20261120.md`（magic 15 条）、`E:/UserFleet/qa/qa-reachability-audit-20261120.md`（四域汇总 + 整改台账 12 行 + 验收总则）、`E:/UserFleet/qa/qa-golden-fixture-drift-disposition-20261120.md`（处置文本）、`E:/UserFleet/qa/qa-rng-total-checklist-20261120.md`（A1/A2/A3 + C/D/E 复核清单）。

**汇总：可达 6 ｜ 未验 8 ｜ 部分不可达 2 ｜ 不适用 26。无"行为面本身不可达"的条目。**

| 域 | 结论 | 消费字段锚 |
| --- | --- | --- |
| magic | 未验 6（MG-10/11/19/27/53/54，均因 `AntiMagic=3`）；部分不可达 2（MG-28/29：52 尾封 `WAbil` 全量，`MaxExp=50` 为夹具构造值，登录给 `GetLevelExp(20)`）；可达 4（MG-12/37/39/40）；不适用 3（MG-32/33/43） | `MagicManager.cs` 对应 case 支的 `targetObject.AntiMagic`；52 尾 `EDCode.EncodeMessage(WAbil)` |
| items | 未验 2（IT-01/03：向量 `max_hp=100/max_mp=80` 与 `level=1` 派生 19/15 冲突）；可达 2（IT-05、IT-18——`max_weight=50/15/12` 恰为 L1 登录值，可作自洽参照）；不适用 4 | `BaseObject.cs` 的 `IncHealthSpell` clamp / Shape3 百分比 / `ItemList.Count` 门 |
| quests | 8/8 不适用（flag/unit 台账；向量无 `LEVEL/EXP/HP/GOLD` 命中） | — |
| npc | 11/11 不适用（脚本 say/label；帧面 643/657/1100 无能力字段；NP-11 观测者用默认能力） | — |

**处置（整改批次：夹具元组自洽化 + 生产可达变体）**

1. **MG-10/11/19/27/53/54**：补**登录序变体**（`AntiMagic=1` + 公式 `MaxHP`），让这 6 条在确定可达的场景下被验一次；合成角色件降为**附加场景**。
2. **MG-28/29**：夹具 `MaxExp` 改用登录派生 `GetLevelExp(20)`；该字段原值生产不可达，**不降级条目**。
3. **MG-37**：`fixture_spec.target_max_hp=100` 与帧面 224 不符 = **stale 元数据**，重生成时按登录派生 224 写。
4. **MG-40**：caster `L30 + MaxHP=100`（真实 L30=419）元组不自洽——改登录派生或显式声明为合成场景，二者择一。
5. **IT-01/IT-03**：向量 `max_hp=100/max_mp=80` 与 `level=1` 冲突 ⇒ 二选一：①改用 L1 登录派生（19/15）；②给出装备/套装依据证明可达（`AddAbil.HP/MP` 来源 `PlayObject.cs:2542/2546/2550/2555/2628`）。两者都不成立才谈降级。

## 7. 验收总则（整改批次的通用判据）

- 整改完成后该条目 `reachability` 必须从 `unverified`/`unreachable` 变为 **`reachable`**，**附构造路径**：`(Job, Level, 装备/套装, 技能/状态) → 字段值`，且该构造要能在 `fixture_spec`/向量元数据里**机检**（字段 + 公式复算）。
- **只改注释或判定词不算通过**；QA 独立重生成/只读核对后方可改状态。
- `legacy_artifact` 件（`Fixture.rng_mode=freeze_at_dospell`）**不得**作为任何条目的 `verified` 依据。
- 任何 `status` 变更单独一笔、走分支、经 QA 签署，禁止与代码/装置改动混提。

## 8. 比较器收紧设计：`Rng` 全轴比较（M52；2026-11-20，opc-07 定）

### 8.1 判据

- **统一比较**：`golden.Rng == rust.Rng ++ rust.DeliverRng`（拼接；`RngSeqs` 连续性断言保留）。
- **不按 seam 字段分两种比法**：所有 `verified` 条目的 golden 必须是 **`cover_drain`**
  （`Fixture.rng_mode == "cover_drain"`）；`freeze_at_dospell` 旧件只作 **legacy artifact**、
  **不进比较面**（比较器直接红，牙③）。
- **缺列即红**：`Rng` 与 `DeliverRng` 任一列缺失都不允许默认空（牙④）——"不记 = 不比"被禁止。
- **适用面**：全 **15 条**（不止那 6 条）；拼接顺序靠实测证、不靠推断；任一条红 ⇒ 报 opc-07，
  不改判据迁就。

### 8.2 四颗牙（合成夹具，不依赖真实台账）

| 牙 | 变异 | 期望 |
|---|---|---|
| ① | 删 `DeliverRng` 一条 | 红（`Rng` 序列不等） |
| ② | 改 `DeliverRng` 任一值 | 红（`Rng` 值不等） |
| ③ | 旧口径 golden（`freeze_at_dospell` / 无 `rng_mode`） | 红并点名 `cover_drain` |
| ④ | 整列 `DeliverRng` 消失 | 红并点名 `DeliverRng` 缺列 |
| ⑤ | `Fixture` 缺 `reachability` / 非 `reachable` / 缺任一状态键 / 缺 `construction` / `derived` 与公式不一致 / 平铺 `target_max_hp` 与 `derived` 不一致 / 装备非空 / 缺 `rng_total_draws` / `len(Rng) ≠ rng_total_draws` | 各自红并点名缺键或字段 |

外加**正对照**：`golden.Rng = rust.Rng ++ rust.DeliverRng` ⇒ 绿（证明判据不是恒红）。

### 8.3 字段契约（交给 `a2`；双方向**不得各自造名字**）

`MagicGolden/Program.cs` 由 **`a2` 独占**；Rust 侧只按下面的键名读。

| 项 | 约定 |
|---|---|
| 键 | **`Fixture.rng_mode`**（每支 case 的 `Fixture` 内；顶层 `fixture_spec` 不需要） |
| 取值 | `"cover_drain"` \| `"freeze_at_dospell"`（只此两个） |
| 缺失 | **比较器直接红**（"金标准缺该支 rng_mode / 非 cover_drain"），**不得默认任何值** |
| legacy artifact | 用 `rng_mode="freeze_at_dospell"` 表示；如需额外的人读标注可加 `Fixture.legacy_artifact=true`（比较器**不看**该键） |
| 语义 | `cover_drain`：`Rng` 覆盖到排水结束，比较面 = `Rng ++ DeliverRng`；`freeze_at_dospell`：**不**进比较面（`verified` 条目禁止使用） |
| 与 `login_order` 的关系 | 两者正交：`rng_mode` 只描述"抽数记录窗口"；`login_order` 描述角色状态来源。**M52 起**：6 条 + `MG-10` 的 verified 依据 = **`login_order=true`**（登录序）+ `cover_drain`（据此撤回早先"`login_order=false`"的中间口径） |

**唯一 `Fixture` schema（M52 起，每支 case 必填；缺任一即红，不许默认兜底）**：

```text
rng_mode        : "cover_drain" | "freeze_at_dospell"
login_order     : true | false
reachability    : "reachable" | "unverified" | "unreachable"
anti_magic      : int
target_level    : int
target_hp       : int
target_max_hp   : int
derived         : { max_hp, max_mp, dc, ac, mac, max_exp }   # 必须与构造路径按公式复算一致
construction    : { job, level, equipment[], skills[] }      # §7 的可机检构造路径
rng_total_draws : int                                        # 整个 case 的真实抽取总数（独立计数器）
scenario_note   : string                                     # login_order=false 必填（人读）
legacy_artifact : true + legacy_reason                       # 仅 freeze_at_dospell 自动写（人读，比较器不看）
```

比较器的判定（`check_golden_contract`）：

1. `rng_mode == "cover_drain"`；`reachability == "reachable"`；`login_order` 为 bool；
   `anti_magic`/`target_level`/`target_hp`/`target_max_hp` 为 int；
2. `construction.{job,level,equipment,skills}` 齐全，且 `construction.level == target_level`；
3. `equipment == []` ⇒ `anti_magic == 1`；**`equipment` 非空 ⇒ fail-closed 红**（装备 MgAvoid 复算未实现）；
4. `derived` 六键齐全；`max_hp/max_mp/dc/ac/mac` 用 `mir2-formula` 的
   `recalc_level_abilitys(job, level, default)` **复算比对**；`target_max_hp == derived.max_hp`；
   `target_hp ≤ max_hp`；**`derived.max_exp` = `GetLevelExp(level)` = 冻结表 `need_exps[level]`**
   （见 8.3b；该字段**有消费点**——MG-28/29 的 52 帧尾封 `WAbil` 全量，含 `Exp/MaxExp`）；
5. `legacy/` 目录：`golden/magic/legacy/**` 只作历史留档，`golden_file` 直接拒绝该前缀。
6. **独立抽取计数器（§8.3c）**：`Fixture.rng_total_draws` 必填（int，缺即红）；`rng_mode=="cover_drain"`
   时 **`len(golden["Rng"]) == rng_total_draws`**，不等即红并点名两值。作用：把"排水期无抽"从假设变成
   被检查的事实（`fireball` 的 5 抽全在施法期、排水 handler 不调 RNG ⇒ 结构上合法），并防"手工截短
   金标准 `Rng` 以对上 Rust"。生成器侧（a2，M52.1）另有 `rng_mode=cover_drain ⇒ Len(Rng)==rng_total_draws`
   断言（不等 rc=2）。

### 8.3b 冻结 `NeedExps` 表（M51 产出，M52 读）

- 文件：`mir2-rs/tests/parity/fixtures/exps-need-exps.json`
- 字段：`{ "source": "E:/MirServer/M2GameSvr/Exps.conf", "sha256": "<64 hex>",
  "mtime": "<取样时文件 mtime>", "need_exps": [NeedExps[0], NeedExps[1], …] }`
  （`need_exps[level]` 即 `SystemShare.Config.NeedExps[level]`；`GetLevelExp` 直接下标取。）
- **同源要求**：golden 生成器（`a2`）与比较器（本侧）读**同一份**文件；它是**冻结面**——
  `Exps.conf` 变化必须显式更新该文件（source/sha256/mtime 同步）并重新生成相关 golden。
- 当前部署取样：`Exps.conf` sha256 = `07d7801cad77082f0a23654771e4e9026d6ef3845214dc96d2593940bf4b71e9`、
  mtime = `2026-10-09 01:32:29 +0800`、`Level20=6000`（供 `a2` 核对；表中以 `Exps.conf` 实值为准）。
- 比较器：缺文件 / `sha256` 非 64 hex / 缺 `source` / 缺 `need_exps` ⇒ **红**（fail-closed）。

### 8.3d `target_hp` 的语义 = **施法前**输入（2026-11-20 实读三条既有 golden）

| 件 | `Fixture.target_hp` | `TargetHpBefore` | `TargetHp`（战后） | 31 帧 `param(HP_after)` / `series(damage)` | 结论 |
|---|---|---|---|---|---|
| `fireball`（旧 runner，无 `Fixture`） | — | 100 | 85（edge/hit） | 85 / 15 | 施法前=100，`param+damage` 复算一致 |
| `hangmajinbub` | 100 | 100 | 100 | 无 31（14 无伤害） | `target_hp` = 战前 |
| `lightning-11` | 100 | 100 | 85 | 85 / 15 | `target_hp` = 战前（复算一致） |

⇒ 既有件的 `target_hp` 一律是**施法前**值。**比较器新增检查**（`check_golden_contract`）：
对每个 `SM_STRUCK(31)` 帧校验 `param(HP_after) + series(damage) == Fixture.target_hp`，不等即红并点名
"target_hp 必须是施法前输入"；无 31 帧（无伤害支）则只保留 `target_hp ≤ max_hp` 的结构检查。

**已知反例（装置侧，交 a2）**：M52.1 样本 `fireball-login.json` 的 `target_hp=209`（= 224−15，**施法后**
值）若当输入使用，会从 209 起手并使所有 HP 字段错位、极易伪装成真分歧 ⇒ 新件必须写 **224**。

**另记**：`hangmajinbub`（及同类）`Fixture.target_max_hp=100` 与 52 尾里的 224 不一致 = 已知 stale
元数据（§6 处置第 3 条），新件按登录派生值写。

### 8.4 实现落点（本设计分支 `card/magic-rng-union`）

- `mir2-rs/tests/parity/tests/magic_fireball_parity.rs`：
  `union_rng` / `check_golden_cover_drain` / `pin_union_rng` / `verify_union` +
  `union_rng_positive_control_green` + `tooth_union_*` 四颗；
  `real_ledger_verify_labels` 在读取每份台账后走这三步（cover_drain 门 → 拼接 → `verify`）。
- **`MagicGolden/Program.cs` 不在本分支改动**（a2 领地；它按 6.2b 输出 `Fixture.rng_mode`）。
- **本分支不合并**：比较器收紧**必须与 6 条新 golden + `MG-10` 同一次合并落地**（M51；中间态
  master 会红）。6 条重生成依赖 `a2` 的 `rng_seam`/`login_order` 参数（M50，批准后实施）。

### 8.5 复跑脚本设计（槽位到手后执行；`run_id` 绑定）

```sh
export MIR2_LEDGER_RUN_ID="magic-plane-$(date +%s)-$$"
for t in fireball_parity_miss fireball_parity_edge fireball_parity_hit          fireball5_parity_miss fireball5_parity_edge fireball5_parity_hit          lightning11_parity_miss lightning11_parity_edge lightning11_parity_hit          lightning45_parity_miss lightning45_parity_edge lightning45_parity_hit          healing_parity_heal healing_parity_full healing_parity_self          firecharm_parity_miss firecharm_parity_edge firecharm_parity_hit          hbfireball_parity_miss hbfireball_parity_edge hbfireball_parity_hit          shield_parity_s1 shield_parity_s5 shield_parity_s13          hangmajinbub_parity_s1 hangmajinbub_parity_s5 hangmajinbub_parity_s13          dejiwonho_parity_s1 dejiwonho_parity_s5 dejiwonho_parity_s13          showhp_parity_miss showhp_parity_edge showhp_parity_hit          cloak_parity_s1 cloak_parity_s5 cloak_parity_s13          big_healing_parity_heal big_healing_parity_full big_healing_parity_both          big_cloak_parity_s1 big_cloak_parity_s5 big_cloak_parity_s13          fireboom_parity_s1 fireboom_parity_s5 fireboom_parity_s13          snowwind_parity_s1 snowwind_parity_s5 snowwind_parity_s13; do
  cargo test -p game-svr --lib -- --ignored --exact "tests::$t" --test-threads=1 || exit 1
done
cargo test --manifest-path tests/parity/Cargo.toml --test magic_fireball_parity -- --ignored || exit 1
```
（每支独立进程 = rng_trace 进程级；任一步非零即停，不重试掩盖。）

### 8.6 与已收口条目的兼容性

- 23/33（即时受击）：受击抽在施法段内、已在 `Rng`；台账 `DeliverRng` 实测为空
  ⇒ 统一判据下 `Rng ++ [] == Rng`，与现行 golden 一致。
- 6 条延时受击：旧 golden 的 `Rng` 缺排水抽（`freeze_at_dospell`）⇒ 重生成后按拼接比较；
  Rust 侧现台账的 `DeliverRng` 已有对应抽（`3b` 判 **18/18 MATCH(count+values)**）。

### 8.7 M52 的登录序变体：状态契约 + 构造路径（Rust 侧已接缝，值待 a2 输出）

**状态字段契约（`MagicGolden` 的 `Fixture`；a2 输出，Rust 只读）**：

| 键 | 类型 | 含义 |
|---|---|---|
| `login_order` | bool | `true` = 走真实登录序（`RecalcLevelAbilitys(); RecalcAbilitys();`，无手设覆盖） |
| `anti_magic` | int | 该 case 的**实际**目标 `AntiMagic`（登录序无装备 ⇒ `1`） |
| `target_level` | int | 目标 `Abil.Level`（构造路径的 L） |
| `target_hp` / `target_max_hp` | int | 该 case 的**实际** HP / `MaxHP`（登录序 ⇒ `MaxHP` = 等级公式值） |
| `rng_mode` | string | `cover_drain` / `freeze_at_dospell`（见 8.3） |

**构造路径（机检口径）**：登录序、无装备、无 `AddAbil` 加成时
`AntiMagic = 1 + 0 = 1`；`MaxHP = 14 + round(((L / WarrHP) + WarrHPRate + L / 20) × L)`，
部署 `LevelValueOfWarrHP=4`、`WarrHPRate=4.5`：

| L | `MaxHP` | 说明 |
|---|---|---|
| 11 | 14 + round(7.8×11) = **100** | 与存量的 `MaxHP=100` 数值相同（但语义是 L11 登录值，不是手设） |
| 20 | 14 + round(10.5×20) = **224** | 与 23/29/33 装置观测到的 224 一致 |

**逐条机检**：比较器/交付里对每条 case 复算：`Fixture.anti_magic == 1`、
`Fixture.target_max_hp == 14 + round(((L/4)+4.5+L/20)*L)`、`Fixture.rng_mode == "cover_drain"`、
`Fixture.login_order == true`；任一不等 ⇒ 该条不能被当作可达证据（§7）。

**Rust 接缝（本分支已写，未编译）**：`fireball_parity_drive` 在建夹具后、魔号专属覆盖前，
按 `Fixture.anti_magic / target_hp / target_max_hp` 设目标实体与会话（缺省 = 老件现状 ⇒
既有台账零变化）。**healing 的专用驱动**（`healing_parity_drive`）待新 healing 金标准落地后
同法接入（它的 `hp` 是显式参数，需要与新 `Fixture.target_hp` 对齐——届时一并改）。

**合成角色件（`AntiMagic=3`/`MaxHP=100`）**：按 §6 处置第 1 条降为**附加场景**，
`rng_mode=cover_drain` 保留作 RNG 轴对照，但 `reachability=unverified`、**不作 verified 依据**。
**并且在新契约下它们已不可再生**：`equipment=[]` 时 `anti_magic` 必须为 1（契约门强制），
`AntiMagic=3` 需非空装备（复算未实现 ⇒ fail-closed）。因此合成件只能作为**历史留档冻结在
`golden/magic/legacy/`**，不再进入任何生成/比较流程——**不要试图"重生成旧件"**。
**旧件冻结后，任何人不得用新装置尝试重生成**：契约门（`equipment=[] ⇒ anti_magic==1`）会拒绝，
重生成只会浪费时间，并可能引出"那就放宽门"的错误提案。

**含 `WAbil` 的帧（`SM_ABILITY(52)`）**：其尾 = `EDCode.EncodeMessage(WAbil)`（40B，`Exp`@24..28、
`MaxExp`@28..32），比较器对 `tail` 做严格串比较 ⇒ 这两个字段**已在判定面内**；牙
`tooth_wabil_exp_bytes_compared_red` 取真实 golden 的 52 尾翻转 `Exp`/`MaxExp` 字节证明必红。
**M53（=旧 M52 顺延）报告须单列**：Exps 装置 bug 修复后若 `max_exp` 由 0 变为真实值，
属**行为类差异**（帧内容变化），不得并入"元数据"；Rust 侧该字段若与 golden 不同 ⇒ **真分歧**，停下报。

**M53 的 Rust 侧范围 = 7 条**：6 条（`lightning-11`/`lightning-45`/`fireball-5`/`firecharm`/`hbfireball`/`healing`）
**加 `fireball`（`MG-10`）**。`fireball.json` 由独立 runner `FireballCase` 产出、当前**无 `Fixture`**；
`a2` 改造该 runner 并在 M52 输出带契约键的新件后，本侧按同一套处理：
`(Warrior, L, 无装备, 无技能) → anti_magic=1、target_max_hp=公式(L)`，逐条给构造路径。
**勿按 a2 上一版迁移表（"fireball 迁移后 cover_drain"）准备**——该声明已撤回，`fireball` 归入
"登录序主件 + 旧件留档"同一套处置。Rust 与新的登录序 golden 对不上 ⇒ **真分歧，停下报**。

**M52 交付自检（必须逐条给出原文）**：① 被合的树里存在
`mir2-rs/tests/parity/fixtures/exps-need-exps.json`
（`git cat-file -e <head>:mir2-rs/tests/parity/fixtures/exps-need-exps.json`，rc=0）；
② 该文件的 `sha256` 与部署 `Exps.conf` 实值一致；③ 全 15 条在同一 `run_id` 下的实测结果。

### 8.8 `AntiMagic=1` 下的 miss/edge/hit 分支可达性（M53 必答项）

**判据**：`DoSpell` 的首抽 `Random(10)` 与 `AntiMagic` 比较（`>=` ⇒ 命中；`<` ⇒ 未命中；`==` ⇒ 边界）。
`AntiMagic=1` 时三支分别要求首抽 **0 / 1 / ≥2**。该首抽是 6 个 AntiMagic 魔号（1/5/11/13/44/45）与
`fireball`（`FireballCase`，同为 `spec.AntiMagic`）的**第一个抽取**（护身符门与 `UseAmulet` 零抽）。

**取数方法（可复核）**：用 .NET 兼容的 `Random(seed).Next(10)` 复刻（经典减法算法）扫描种子，
并**用已入库 golden 记录的三个 `(seed, 首抽)` 校准**：`seed 1 → 2`、`seed 5 → 3`、`seed 13 → 5`
（与 `fireball.json`/`anti_magic=3` 的 miss/edge/hit 一致，校准通过）。

| 分支 | `Random(10)` 要求（AntiMagic=1） | 候选 seed | 该 seed 首抽 |
|---|---|---|---|
| miss | `< 1`（即 0） | **14** | 0 |
| edge | `== 1` | **18** | 1 |
| hit | `>= 2` | **1** | 2 |

⇒ **三支在登录序场景下均可达**，不存在"某支不可达、覆盖率须收窄"的情形（`Random(10)=0/1` 的概率各 10%）。
**最终以 M53 的实跑为准**：新种子由 `a2` 的生成器写入 golden `Fixture.seed`；本侧测试从
`Fixture.seed` 读 **输入**，帧面**期望**只来自 golden 帧序列与比较器判据（绝不反向从 golden 构造期望）。
