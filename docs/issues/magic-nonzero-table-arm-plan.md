# 计划：非零值魔表对照臂（装置盲区 B16）

> **只读规划卡（docs-only）**：不实现、不跑构建、不动既有 golden。实现另开分支，由 opc-19 派。
> 起因（B16，`10` 核查 / `a2` 建议 / opc-19 排期）：对拍的 `MagicInfo` 是**装置零值构造**
> —— C# `MagicGolden/Program.cs` 的 `new MagicInfo { MagicId, MagicName, Effect, EffectType }`
> （其余默认 0）、Rust `fireball_rig` 的 `..Default::default()`；而**部署侧**
> `src/GameSrv/DB/MySqlDB.cs` 的 `LoadMagicDB` 把 `TrainLv` **恒置 3**、`DefPower/DefMaxPower/Power/MaxPower`
> 取自 `sql/mir2_data.sql` 的 `magics` 行。**凡经 `GetPower`/`GetPower13`/`MPow` 的魔号，
> 今天"两侧一致"只在零值口径下成立。**
> 背景数（opc-19 转述）：同一参数代入，装置口径 `P(30)=80`（golden 值）、部署口径 ≈54。
>
> 本卡沿用「ServerConf 非缺省对照臂」的成功形态：**先做差异清单，再谈对拍**。

## 1. 影响面（本卡最重要的一节）

### 1.1 读取点（符号锚，不写行号）

| 读取点 | 公式要点 |
| --- | --- |
| `MagicManager.MPow(UserMagic)` / `MagicBase.MPow` | `Power + Random(MaxPower - Power)` ← **Power/MaxPower** |
| `MagicManager.GetPower(UserMagic, nPower)` / `MagicBase.GetPower` | `Round(nPower/(TrainLv+1)*(Level+1)) + DefPower + Random(DefMaxPower-DefPower)` ← **TrainLv / DefPower / DefMaxPower** |
| `MagicManager.GetPower13(UserMagic, nInt)` / `MagicBase.GetPower13` | `Round(d18/(TrainLv+1)*(Level+1) + d10 + DefPower + Random(DefMaxPower-DefPower))`，`d10=nInt/3`、`d18=nInt-d10` |
| `PlayObject.Attack` 的武器技能支（`SKILL_ONESWORD`/`ILKWANG`/`YEDO`/`ERGUM`/`BANWOL`/`FIRESWORD`…） | 读 `TrainLv` 做等级门；**不在魔法施法路径**，本卡不改它，列此仅为界定 |

**结论**：`TrainLv` 同时是"威力公式的分母"与"武器技能等级门"的输入；本卡只覆盖**施法路径的威力公式**。

### 1.2 走这些读取点的 case（`MagicManager.DoSpell` 的 `case MagicConst.*`）

**直接调用（25 个 case）**：`SKILL_FIREBALL`、`SKILL_FIREBALL2`、`SKILL_FIRE`、`SKILL_SHOOTLIGHTEN`、
`SKILL_LIGHTENING`、`SKILL_45`、`SKILL_FIRECHARM`、`SKILL_FIREBOOM`、`SKILL_47`、`SKILL_LIGHTFLOWER`、
`SKILL_SNOWWIND`、`SKILL_EARTHFIRE`、`SKILL_MABE`、`SKILL_HEALLING`、`SKILL_BIGHEALLING`、
`SKILL_AMYOUNSUL`、`SKILL_SHOWHP`、`SKILL_SHIELD`、`SKILL_CLOAK`、`SKILL_BIGCLOAK`、
`SKILL_HANGMAJINBUB`、`SKILL_DEJIWONHO`、`SKILL_HOLYSHIELD`、`SKILL_UENHANCER`、`SKILL_TWINBLADE`。

**经 helper（`case` 体里看不到 `GetPower` 也要算）**：
- `MagGroupAmyounsul`（`SKILL_GROUPAMYOUNSUL`，48）→ `MagicBase.GetPower13(40/30, …)`；
- `MagGroupLightening`（`SKILL_GROUPLIGHTENING`，42）→ `MagicBase.GetPower(MPow, …)`；
- `MagHbFireBall`（`SKILL_HBFIREBALL`，44）→ `MagicBase.GetPower(MPow, …)`；
- `MagMabMabe`/`MabMabe`（50）在 `case SKILL_MABE` 直接调用（已在直接清单内）。

**有 SQL 值但路径不读**（不算受影响，避免误判）：`MagGroupDeDing`（39）用 `GetAttackPower(WAbil.DC)`、
`MagPushArround`（8/37）不读魔表威力、召唤/位移类（16/17/21/30/55/56…）不读。

### 1.3 真值来源与解析语义（**易错点**）

`LoadMagicDB` 的执行序：`select * from magics`（**无 ORDER BY**）→ 逐行 `new MagicInfo{}` →
`TrainLv = 3`（恒置）→ `AddMagicList(magic)`（`List.Add`，不去重）；
`WorldServer.FindMagic(int nMagIdx)` **返回列表里第一条命中**。所以：

- **同一 MagID 有多行时，生效的是"最先被读到"的那一行**（InnoDB 通常按主键 `Idx` 升序返回）。
- **实例（必须写进夹具来源说明）**：MagID 48 有两行 —— `Idx=38` 的 `群体施毒术`
  （`Power=10, MaxPower=10, DefSpell=30, DefPower=4, DefMaxPower=4`）与 `Idx=48` 的 `气功波`
  （`Power=1, MaxPower=1, Def=0`）；**`FindMagic(48)` 得到第一行（Idx=38）**，即 B16 引用的
  `DefPower=4/DefMaxPower=4`。
- 结论：**真值必须按"`Idx` 最小且 `MagID` 匹配"的那一行取**，不能拿任意同 MagID 行；
  夹具里要记 `Idx`，否则重生成时无法自证取的是哪一行。

### 1.4 受影响集合（`sql/mir2_data.sql` 统计，108 行）

- `DefPower` 或 `DefMaxPower` > 0 的**不同 MagID**：41 个
  （1,5,9,10,11,13,22,23,24,29,33,36,37,39,44,45,47,48,51,53,58,59,69,76,77,78,79,80,81,82,83,84,85,86,87,91,92,94,96,97,98）。
- `Power` 或 `MaxPower` > 0 的**不同 MagID**：50 个（上述之外的 2,8,20,41,46,49,50,54,55?,500 等）。
- **区间非零**（`MaxPower>Power`，MPow 的 `Random` 模数从 0 变正）：2(14–20)、11(14–28)、13(15–28)、
  24(10–30)、36(12–24)、37(14–25)、44(14–25)、45(21–30)、47(10–30)、53(4–10)、58(30–50)、
  59(15–28)、69(75–90)、91/92/94/96–98（英雄变体）。
- **区间非零**（`DefMaxPower>DefPower`，GetPower 的 `Random` 模数从 0 变正）：24(10–30)、37(10–12)、
  47(10–30)、53(3–6)、69(25–35)、96–98(12–20)。

### 1.5 现有 stem 谁踩线

| 现有 stem（魔号） | SQL 值（取生效行） | 今天装置口径 | 结论 |
| --- | --- | --- | --- |
| `fireball`(1)、`fireball-5`(5)、`fire`(9)、`skill10`(10)、`lightning-11`/`-45`(11/45)、`firecharm`(13)、`fireboom`(23)、`lightflower`(24)、`snowwind`(33)、`hbfireball`(44)、`fireboom47`(47)、`groupamyounsul`(48)、`mabmabe`(50: Power=MaxPower=1、Def=0) | 非零 | 0 | **受影响**（今天的"一致"只是零值一致） |
| `amyounsul`(6)、`shield`(31)、`spacemove`(21)、`skeleton`(17)、`sinsu`(30)、`holyshield`(16)、`groupdeding`(39)、`firewind`(8)、`energyrepulsor`(37) | 6 全 0；31/21/17/30/16 不读；39 走 DC；8/37 推挤不读威力 | 0 | 不受这三个字段影响（37 的 SQL 行虽有 Def，但 case 不读） |
| `skill49/51/52/53/54` 共享尾 | 已经显式注入 `magic_train_lv=3`、`magic_max_train0` | TrainLv=3 | **TrainLv 维度已部分覆盖**（只覆盖训练门，不覆盖威力公式） |

## 2. 臂的形态

### 2.1 为什么是 `MagicInfo` 注入，不是配置夹具

魔表来自**数据库装载**（`LoadMagicDB`），不是 `Server.conf`；ServerConf 臂的"冻结 conf 夹具"形态
在这里没有对应物。**注入点必须是对拍装置构造 `MagicInfo` 的两侧**：

- C#：`MagicGolden/Program.cs` 的 `BuildMagicInfo(MagicSpec)`（已有 `MagicTrainLv`/`MagicMaxTrain0`
  先例：仅当非 0 时写入，缺省不写 ⇒ 旧件零变化）；
- Rust：`fireball_rig` 里 `MagicTable::new(vec![MagicInfo { magic_id, effect, effect_type, ..default() }])`
  （共享尾驱动在 `skill_tail` 路径已从 golden 读 `magic_train_lv`/`magic_max_train0`，可复用同一模式）。

### 2.2 字段与 golden 键（Arm A：五个字段）

| 字段（两侧同名） | golden `Fixture` 键 | 缺省 | 说明 |
| --- | --- | --- | --- |
| `MagicInfo.Power` | `magic_power` | 0 | `MPow` 左端点 |
| `MagicInfo.MaxPower` | `magic_max_power` | 0 | `MPow` 右端点（`Random(Max-Power)`） |
| `MagicInfo.DefPower` | `magic_def_power` | 0 | `GetPower` 的固定项 |
| `MagicInfo.DefMaxPower` | `magic_def_max_power` | 0 | `GetPower` 的区间上界 |
| `MagicInfo.TrainLv` | `magic_train_lv`（已有） | 0 | 分母 `TrainLv+1` |

**零变化保证**：五键全部"非零才写 / 缺键=0"；既有 9 支 golden 不含新键 ⇒ 生成器与驱动行为逐字节不变
（与 `magic_train_lv` 的既有门控同款）。**Rust 侧必须逐键读 golden、缺键=0**，不得默认成部署值。

### 2.3 真值（至少一组，全部取自 `sql/mir2_data.sql` 的生效行）

| MagID | 名称 | Power | MaxPower | DefPower | DefMaxPower | Arm 后 `GetPower`（`Level=1`，代入同 `nPower`） |
| --- | --- | --- | --- | --- | --- | --- |
| 48 | 群体施毒术（Idx=38） | 10 | 10 | 4 | 4 | `Round(MPow/4*2)+4+Random(0)`，`MPow=10` ⇒ **9**（装置：0） |
| 1 | 火球术 | 8 | 8 | 2 | 2 | `MPow=8` ⇒ `4+2` = **6**（装置：0） |
| 11 | 雷电术 | 14 | 28 | 10 | 10 | `MPow∈[14,28]` ⇒ `Round(MPow/2)+10` ∈ **17–24**（装置：0） |
| 24 | 地狱雷光 | 10 | 30 | 10 | 30 | `MPow∈[10,30]` ⇒ `Round(MPow/2)+10+Random(20)`（装置：0，且 `Random(0)`） |

`TrainLv=3` 是**所有**行的公共真值（`LoadMagicDB` 恒置），Arm A 的每个新 stem 都带。

### 2.4 Arm B（整行）为什么先不做

整行注入还会带入 `DefSpell`（`GetDefSpell` 的耗蓝）、`TrainLevel`/`MaxTrain`（升级门与 640 尾）、
`Delay`（施法冷却）、`Effect/EffectType`（SM17 尾串）、`Job`。这些会改变**抽数结构与帧面语义**
（不只是数值），blast radius 大；本卡先做 Arm A（威力公式五字段），把 B 登记为后续（若 Arm A
的差异清单显示威力面已被充分覆盖，B 可延后或不做）。

## 3. 判据与 stem 规划

- **新增 stem，不动既有**：只新增（不复用、不重生成）以下三支（magic id + 真值见 §2.3）：
  - `fireball-nonzero`（1）：覆盖 `TrainLv` 取整 + `DefPower` 固定项；
  - `lightning11-nonzero`（11）：覆盖 `MPow` 区间（`Random(14)`）；
  - `lightflower-nonzero`（24）：覆盖 `DefPower..DefMaxPower` 区间（`Random(20)`）+ AoE 帧面。
  （`groupamyounsul-nonzero`(48) 可选第四支，用于对齐 B16 的原始例子；若前三支已覆盖差异类型，可省。）
- **每支一个"零值对照"**：照 ServerConf 臂的 `--nondefault-control` 形态加 `--nonzero-control`
  （同 label/seed、五键全 0），**先产出差异清单**，再谈对拍。
- **既有 9 支 golden 一律不重生成**（今天反复强调的止损线）；零漂移用 base/new 生成器快照复证。
- 比较器的十键/绝对值表**只对新增 stem 生效**（per-stem 期望表，同 ServerConf 臂的做法）。

## 4. 预期产出：差异类型与"必须有别"

**可能抓出的产品分歧类型**：
1. **取整**：`Round(nPower/(TrainLv+1)*(Level+1))` 在 `TrainLv=0` 与 `3` 下的分母不同 ⇒ 整数量级差
   （`fireball` 例：0 vs 6）；
2. **阈值/下限**：`+ DefPower` 固定项（0 vs 2/4/10）；
3. **模数**：`Random(MaxPower-Power)`、`Random(DefMaxPower-DefPower)` 从 `Random(0)` 变为正区间 ⇒
   **同 seed 抽值不同**（抽数条数不变，值全变）；
4. **帧面**：伤害/HP/31/53/100 等帧的数值与（可能的）条数；
5. **经验/训练**：仅当 Arm B 带入 `TrainLevel/MaxTrain` 才涉及（本卡不做）。

**"两支输出必须真的有别"**：每支新 stem 必须先给出 arm vs control 的差异清单（Rng 值、HP、
帧多重集、关键帧参数），**任一新增支若在所有有意义字段上与零值对照无差异 ⇒ 该支无判别力，换值/换魔号重出**，
不得把它当"已覆盖"。差异清单要落到卡里/提交信息里，不能只写"已验证"。

## 5. 成本与写集（预估）

| 项 | 文件 | 规模 |
| --- | --- | --- |
| C# 装置 | `MagicGolden/Program.cs`：`MagicSpec` 四新字段 + `BuildMagicInfo` 写入 + `--case *-nonzero` 分支 + `--nonzero-control` | ~120 行 |
| Rust 驱动 | `bins/game-svr/src/lib.rs`：`fireball_rig` 读五键（缺键=0）+ 3 支 `#[ignore]` 驱动 + per-stem 期望表 | ~100 行 |
| Rust 比较器 | `tests/parity/tests/magic_fireball_parity.rs`：3 个 `real_rust_ledger_*_nonzero`（或 1 个参数化 wrapper） | ~30 行 |
| golden | `golden/magic/{fireball,lightning-11,lightflower}-nonzero.json`（3 份新增） | 新增 |
| 产品代码 | —— | **0**（`MagicInfo` 只写字段，不改结构） |

门集合变化：**Driver +9**（3 支 × 3 label）、**Comparator +3**；`changed ids` **空**（辅助 stem，不增 MG 条目）。

## 6. 止损线与不做

- **不新增 MG 条目**、不改 `docs/issues/parity-gap-magic.json`；
- **不动产品代码**（`MagicInfo` 结构、`MagicManager`/`MagicBase`/Rust `world.rs` 均不改）；
- **不动既有 9 支 golden**；零漂移必须用 base/new 生成器快照复证并贴命令+输出；
- **不 `#[ignore]` 掩盖红**：若新支暴露产品分歧，按 ServerConf 臂的先例**拆批 + 单独立卡**，
  本批只合"能绿的"；
- 若需要改 `MagicInfo` 公共结构或改 `LoadMagicDB`/`FindMagic` 语义 ⇒ **停手报 opc-19**；
- 若五键注入导致旧 stem 出现任何漂移 ⇒ 停手，先查门控是否"非零才写"。

## 7. 落地顺序（待批）

1. 建分支（base 取当时 master）；
2. C# `MagicSpec`/`BuildMagicInfo` 五键门控 + `--nonzero-control`（旧件零变化，单独提交）；
3. Rust `fireball_rig` 五键读取（缺键=0）+ 3 支驱动；
4. 生成 3 份 golden（先产**差异清单**，确认每支与零值对照确有差异）；
5. 比较器 per-stem 期望表（仅新 stem）；
6. 变异牙：把 Rust 侧五键读取改成"恒 0" ⇒ 3 支驱动/比较器必红，且**既有 9 支保持绿**；
7. 门：fmt / clippy / `game-svr --lib` / `serverconf_parse_parity`（保持 2 passed）/
   `--no-run -p mir2-parity-tests` / `parity-gate -Mode Both`（新 run_id）；
8. 零漂移复证 + 送审（贴命令与输出原文）。
