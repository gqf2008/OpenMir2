# 计划：MG-48「非缺省值」行为区分支（配置读取的装置缺口）

> **只读规划卡（docs-only）**：不建分支、不改部署 `Server.conf`、不重出既有 golden。
> 前置状态（2026-11-21）：MG-44/48 升格合并**已完成**，`master = ec9473e50`（覆盖率 89/97；
> 11029 评估卡已合）。MG-42 的合并门在跑（负责人本地尚未推）——本卡落地等它门绿回填后，
> base 取**当时的 master**（不写死本文的 sha）。
> 起因：`3b` 预核指出 —— `MabMabe*` 四键的**部署值与 `GameSvrConf` ctor 缺省完全相同**
> （100/10/21/20），因此「读了配置」与「吃了缺省」在 MG-48 上**行为等价**；现有判据只有
> `server_conf` 十键绝对值 + 生成源 sha 钉死（`magic-serverconf-batch-impl.md` §2 已登记该边界）。
> 本卡把这条边界补成**行为面可区分**的一支。

## 0. 落地状态（2026-11-21，opc-19 裁定后）

- **已落地（本批）**：`EnsureConfFiles` 身份修复 + `--conf-selftest`、非缺省夹具、`--case
  mabmabe-nondefault` 分支（seed 合取扫描 + 三层断言 + `--nondefault-control`）、新 golden stem
  `mabmabe-nondefault`；§8.3d 判据改为**总量恒等式**（附等价性牙，证明旧判据能抓的变异新判据仍红）。
- **推迟（随产品卡修复一起落地）**：`mabmabe-nondefault` 的 Rust 驱动 3 支、比较器
  `real_rust_ledger_mabmabe_nondefault`、per-stem 期望表。原因：该臂首次覆盖 50 **成功臂**，
  抓到 Rust/C# 真实分歧，见
  [`openmir2-mabmabe-success-arm-parity.md`](openmir2-mabmabe-success-arm-parity.md)。
  裁定：master 不留红、**禁用 `#[ignore]` 掩盖**，故驱动/比较器不进本批。

## 0.1 结论先行

- **MG-44 不需要**这条支：`MagTurnUndeadLevel` 部署 44 ≠ ctor 缺省 50，已有即死结果牙。
- **MG-48 需要**：用一份**独立的冻结夹具 `Server.conf`**（四键取**第三值**：既不等于部署值、
  也不等于 ctor 缺省）重出一支 `mabmabe-nondefault`。
- 产品代码 **0 处**改动；只动装置（C# 2 文件）+ 驱动/比较器（Rust 2 文件）+ 夹具/golden。
- 顺带修一个**装置既有缺陷**：`EnsureConfFiles` 只在目标文件缺失/0 字节时才拷贝（另卡
  [`openmir2-ensureconffiles-stale-copy.md`](openmir2-ensureconffiles-stale-copy.md)）。

## 1. 改哪几个键、改成什么、为什么能区分（问 1）

### 1.1 值表（四键全部取「第三值」）

| golden 键 | oracle `[Setup]` 键 | 部署值 = ctor 缺省 | **本支取值** | 读取点 |
| --- | --- | --- | --- | --- |
| `mab_mabe_hit_rand_rate` | `MabMabeHitRandRate` | 100 | **50** | `MabMabe` 的门①：`Random(HitRandRate) < MAX(MinLvLimit, nLevel*8-nLevel+15+nLv)` |
| `mab_mabe_hit_min_lv_limit` | `MabMabeHitMinLvLimit` | 10 | **40** | 同上 `MAX(...)` 的右操作数下界 |
| `mab_mabe_hit_sucess_rate` | `MabMabeHitSucessRate` | 21 | **90** | 门②：`Random(HitSucessRate) < nLevel*2+4` |
| `mab_mabe_hit_mabe_time_rate` | `MabMabeHitMabeTimeRate` | 20 | **7** | 成功臂 STONE 时长：`nPower / MabeTimeRate + Random(nLevel)` |

其余六键在夹具里保持部署值（`MagTurnUndeadLevel=44`、`AmyOunsulPoint=15`、`MagTamming*=40/1/100/5`）。

### 1.2 为什么"第三值"能区分

设本支施法者 `Level=30`、玩家目标 `Level=20`、`UserMagicLevel=1`（与现有 `mabmabe` 一致）：

- `nLevel = 1`、`nLv = 10` ⇒ `nLevel*8-nLevel+15+nLv = 32`；
  - **本支**：`MAX(40, 32) = 40` ⇒ 门① 为 `Random(50) < 40`（通过率 80%）；
  - **吃缺省**：`MAX(10, 32) = 32` ⇒ 门① 为 `Random(100) < 32`（通过率 32%）。
- **本支**：门② 为 `Random(90) < 6`（6.7%）；**吃缺省**：`Random(21) < 6`（28.6%）。
- **本支**：STONE 时长 `nPower/7 + …`；**吃缺省**：`nPower/20 + …`。

三条独立区分通道：①模数（同一 seed 下取值不同 ⇒ `Rng[i]` 逐值红）；②门结果翻转（帧面/HP）；
③时长系数（`RM_POISON` 帧参数）。

**为什么不用 0/负数**：两端都 fail-closed 拒绝 `≤0`，那是"配置缺失/破损"的判据，不是"读取路径"。
**为什么不改 `mag_tamming_*`**：MG-48 路径不消费，无行为面。

### 1.3 配置来源：独立冻结夹具，不动部署文件

- 新增 `mir2-rs/tests/parity/fixtures/serverconf/nondefault/`：`Server.conf`（部署件逐字节复制后
  只改上表四键）+ 其余四份 conf 逐字节复制（`EnsureConfFiles` 需要五份齐全）。
- C# `ServerConfFixture` 增加 profile：`Deploy`（现有十键期望 + 现有 sha）与
  `NonDefault`（四键取本支值 + **夹具件独立 sha**）；仍按「先 sha、后逐键绝对值」断言。

## 2. 取数序（已用真装置 golden 逐抽核对，问 "扫描器怎么保证"）

### 2.1 实际取数序（`MagicGolden` 装置带 trace 的拷贝实测）

魔法 50 从 `Install(seed)` 起到判据相关抽数（`s`/`r` 为同一 seed 的样本序）：

| # | 模数 | 调用点（符号） | 说明 |
| --- | --- | --- | --- |
| 1 | `Random(0)` | `MagicManager.MPow(userMagic)` | `MagicInfo` 的 Power/MaxPower 全 0 ⇒ `Random(0)`（**仍消耗一抽**） |
| 2 | `Random(0)` | `MagicManager.GetPower(userMagic, mPow)` | 同上 |
| 3 | `Random(nPower+1)` | `PlayObject.GetAttackPower`（DoSpell 的 `SKILL_MABE` 臂） | `nPower = HiByte(MC)-LoByte(MC)+1`；非 MC 变体标签（MC=15）⇒ `-14` 被钳 0 ⇒ `Random(1)` |
| 4 | `Random(10 - MAX(0,-Luck))` | `PlayObject.GetAttackPower`（同一次调用的幸运尾） | 装置 `Luck=0` ⇒ `Random(10)`；`Random(10-MIN(9,Luck))` 是 `Luck>0` 的另一半 |
| 5 | `Random(10)` | `MagicManager.MabMabe` 的 AntiMagic 门 | 要求 `AntiMagic(3) <= v` |
| 6 | `Random(2)` | `MabMabe` | `Random(2)+(Level-1) > target.Level`（30/20 ⇒ 恒真） |
| 7 | `Random(HitRandRate)` | `MabMabe` 门① | 本支 50 / 吃缺省 100 |
| 8 | `Random(HitSucessRate)` | `MabMabe` 门② | 本支 90 / 吃缺省 21 |

成功臂之后还有 `GetMagStruckDamage` / `StruckDamage` 的排水期抽数（`RM_DELAYMAGIC` 结算），
它们进 golden 的 `Rng` 但不参与翻转判据。

**核对证据（回放验证）**：按上表回放 `new Random(seed)`，与三份现有 `mabmabe` golden 的 `Rng` 前缀逐值相等：

- `s1`（seed=1，**非** MC 变体）：golden `[0,0,0,7,6,0,35,…]` = 回放 `Random(0),Random(0),Random(1),Random(10),Random(10),Random(2),Random(100)`
  —— 第 7 抽 35 ≥ 32 ⇒ 门①不过（golden 的 HP 100→95 是第一条 `RM_DELAYMAGIC` 的 `nPower/3` 结算，不是成功臂）；
- `s5`（seed=5，MC 变体 `MakeWord(5,10)` ⇒ 第 3 抽 `Random(7)`）：golden `[0,0,1,6,4,1,14,19,…]` 与回放逐值相等
  （14<32 过门①、19≥6 不过门②）；
- `s13`（seed=13，MC 变体）：golden `[0,0,5,3,0]` 与回放逐值相等（第 5 抽 0 ⇒ AntiMagic 门不过）。

**装置坑（写进落地清单）**：`RunMagicCase` 的 MC 变体是**按 label 名**判的
（`useMcVariant = CasterMcNoClamp > 0 && (label == "s5" || label == "s13" || label == "hit")`）。
本支 label 命名 `nondef_*` ⇒ **不是** MC 变体 ⇒ 第 3 抽恒 `Random(1)`。扫描器必须按同一口径算；
若将来改成 MC 变体，第 3 抽模数要同步改成 `Random(7)`。

**对先前引用的更正**：`:707 Random(4-L)`、`:717 Random(2)`、`:721 Random(3)`、`:723 Random(L+20+magicLevel*5)`、
`:736 Random(n14)`、`:779/:787/:795 Random(14/20/20)` 这些行在 `MagicManager.cs` 里属于
**`MagTamming`（魔法 20 诱惑之光）**，不是 `MabMabe`（魔法 50）；MG-48 的取数点以本表为准。

### 2.2 扫描器（关键 20 行）

```csharp
static (int a10, int c50, int c100, int d90, int d21, bool anti) Draw(int seed)
{
    var arm = new Random(seed);          // 正确实现：门①=50 / 门②=90
    arm.Next(0); arm.Next(0); arm.Next(1); arm.Next(10);   // MPow / GetPower / GetAttackPower ×2
    int a = arm.Next(10);                // AntiMagic 门
    arm.Next(2);                         // Random(2)
    int c50 = arm.Next(50);              // 门①
    int d90 = arm.Next(90);              // 门②

    var def = new Random(seed);          // 吃缺省实现：门①=100 / 门②=21
    def.Next(0); def.Next(0); def.Next(1); def.Next(10);
    def.Next(10); def.Next(2);
    int c100 = def.Next(100);
    int d21 = def.Next(21);
    return (a, c50, c100, d90, d21, a >= 3);
}
```

（两支都从**同一 seed** 起跑，共享前 6 抽，只有第 7/8 抽的模数不同。）

### 2.3 谓词与互斥

```
anti             = a10 >= 3
arm_success      = anti && c50 < 40 && d90 < 6
default_gate1_stop = anti && !(c100 < 32)          // 缺省停在门①（它连门②都到不了）
default_gate2_pass = anti && c100 < 32 && d21 < 6  // 缺省过了门①且过了门②
randrate_flip    = arm_success && default_gate1_stop
sucess_flip      = anti && c50 < 40 && c100 < 32 && !(d90 < 6) && d21 < 6
```

**互斥**：`randrate_flip` 要求 `default_gate1_stop`（缺省门①**不过**），`sucess_flip` 要求
`c100 < 32`（缺省门①**过**）——两者不能同时为真；且前者要求 `d90 < 6`、后者要求 `d90 >= 6`。
所以它们是同一判据下的两个互斥分类，不是两条独立列表。

### 2.4 扫描实测（每 seed 一行，显式布尔）

```
seed=37 anti=True a10=9 c50=24 c100=48 d90=0 d21=0 | arm_success=T default_gate1_stop=T default_gate2_pass=F randrate_flip=T sucess_flip=F
seed=39 anti=True a10=8 c50=19 c100=38 d90=0 d21=0 | arm_success=T default_gate1_stop=T default_gate2_pass=F randrate_flip=T sucess_flip=F
seed=41 anti=True a10=7 c50=13 c100=27 d90=1 d21=0 | arm_success=T default_gate1_stop=F default_gate2_pass=T randrate_flip=F sucess_flip=F
seed=43 anti=True a10=6 c50=8  c100=17 d90=1 d21=0 | arm_success=T default_gate1_stop=F default_gate2_pass=T randrate_flip=F sucess_flip=F
seed=45 anti=True a10=5 c50=3  c100=6  d90=1 d21=0 | arm_success=T default_gate1_stop=F default_gate2_pass=T randrate_flip=F sucess_flip=F
seed=57 anti=True a10=9 c50=22 c100=44 d90=3 d21=0 | arm_success=T default_gate1_stop=T default_gate2_pass=F randrate_flip=T sucess_flip=F
seed=59 anti=True a10=8 c50=17 c100=34 d90=3 d21=0 | arm_success=T default_gate1_stop=T default_gate2_pass=F randrate_flip=T sucess_flip=F
seed=79 anti=True a10=8 c50=15 c100=30 d90=6 d21=1 | arm_success=F default_gate1_stop=F default_gate2_pass=T randrate_flip=F sucess_flip=T
seed=81 anti=True a10=7 c50=9  c100=19 d90=7 d21=1 | arm_success=F default_gate1_stop=F default_gate2_pass=T randrate_flip=F sucess_flip=T
seed=83 anti=True a10=6 c50=4  c100=9  d90=7 d21=1 | arm_success=F default_gate1_stop=F default_gate2_pass=T randrate_flip=F sucess_flip=T
seed=99 anti=True a10=9 c50=13 c100=26 d90=9 d21=2 | arm_success=F default_gate1_stop=F default_gate2_pass=T randrate_flip=F sucess_flip=T
seed=101 anti=True a10=8 c50=7 c100=15 d90=10 d21=2 | arm_success=F default_gate1_stop=F default_gate2_pass=T randrate_flip=F sucess_flip=T
```

选取（各支独立 seed，避免同一 seed 的耦合）：
`nondef_hit` = **41**（本支成功、缺省也成功——纯"本支行为"对照）；
`nondef_randrate_flip` = **37**（本支成功、缺省停在门①）；
`nondef_sucess_flip` = **79**（本支停在门②、缺省过门②）。

> **前版 seed 候选作废（模型错）**：本卡早期草稿给的 `hit=7/49`、`randrate=7/21`、
> `sucess=12/59` 等值基于**漏了前 4 抽**的错误模型（把门放在 `Rng[0..3]`），一律不得使用；
> 以本节 37/41/79（对应 `Rng[4..7]` 门位）为准。

### 2.5 三层"不是碰运气"保证

1. 扫描器判的是 `arm_success`/`default_gate1_stop`/`default_gate2_pass` 的**合取式**，不是"本支过就行"；
   任一 seed 不满足 ⇒ 换 seed；全范围找不到 ⇒ `FixtureContractViolation`、rc=2、**不落 golden**。
2. golden 落盘前对**记录值**再做 meta-assertion：用 `Rng` 的下标（0 基）`Rng[4]`=AntiMagic、
   `Rng[5]`=Random(2)、`Rng[6]`=门①、`Rng[7]`=门② 重算上面三个布尔量，必须与扫描期一致
   （取数序/.NET RNG/label-MC 口径任一漂移 ⇒ 生成期红，而不是翻转支静默退化）。
3. 行为面另有一层：翻转支的帧多重集（第二条 `RM_DELAYMAGIC` / `RM_POISON` / `SM_STRUCK`）与
   `TargetHp` 结构不同，不依赖数值巧合。

### 2.6 `useMcVariant` 的标签名开关：三层，各管一件事

`RunMagicCase` 的 MC 变体**按 label 名**判（`s5`/`s13`/`hit`），这是"改个 label 就静默改变取数序"的开关。
落地时装置侧加三层，**不是并列的两个断言**（少任何一层都有一类漏）：

1. **判定层（无概率洞）**：抽出 `bool IsMcVariantLabel(string label)` 供 `RunMagicCase` 与扫描器共用；
   本支分支里 `if (IsMcVariantLabel(label)) throw new FixtureContractViolation(...)`。
   它直接判"标签是否属于 MC 变体集合"，与具体抽值无关。
2. **记录值自检层（防判定与记录脱节）**：`nondef_*` 标签下 `Rng[2]`（0 基第 3 抽）必须为 0
   （`Random(1)` 恒 0）。它的作用不是判定（`Random(7)` 对约 1/7 的种子也为 0 ⇒ 单靠它是概率牙），
   而是核"实际落盘的抽数与判定层的结论一致"——若判定层被绕过而运行期走了 MC 变体，记录值会露馅。
   对选定种子实测：`seed=37 ⇒ Next(7)=4`、`seed=41 ⇒ 2`、`seed=79 ⇒ 6`（均 ≠0）。
3. **证明层（负向对照）**：临时把某支 label 改名 `hit` ⇒ 第 1 层必红（第 2 层对上述种子也红）；
   改名还原后复跑绿。它证明前两层真的会咬，而不是"写了没接上"。

## 3. 对既有 golden / 台账的影响（问 2）

- **golden**：只新增 `golden/magic/mabmabe-nondefault.json`；既有 magic golden 一字不改。
- **台账**：新 stem ⇒ `mabmabe-nondefault_<label>.json` + `.run.json`，不覆盖 `mabmabe_{s1,s5,s13}.*`。
- **`parity-gate.ps1 -Mode Both`**：驱动 +3、比较器 +1；`PREEXISTING_LEDGERS` 记的是旧 run_id，
  新侧车按本轮产物处理；驱动与比较器必须同一 `MIR2_LEDGER_RUN_ID`（既有防空转约束）。
- **`serverconf_parse_parity`**：夹具清单是显式数组，新目录不会被它扫到（也不应扫到）。

## 4. 预期红在哪（问 3）

若产品侧真的"吃缺省"（`set_b4_config` 被删/绕开，或 `MabMabe` 读硬编码常量），同一 seed 下：

1. **`nondef_randrate_flip`（seed=37，首选判据，行为面）**：缺省门①不过 ⇒ 产品**少**
   "第二条 `RM_DELAYMAGIC`（伤害值）+ 受击/中毒帧"整段
   （第一条 `RM_DELAYMAGIC`（`nPower/3`）在随机门**之前**入队，不是翻转信号）：
   `Rm 多重集不等（ident,wParam,p1,p2,p3,priority,late）： golden 有 rust 无=[8021, ...]`；
   数值面同时叠加 `[nondef_randrate_flip] RNG[6] 不等：golden=24 rust=48` 与
   `RNG 行数不等：golden=N rust=M`（缺省在门①停 ⇒ 少门②及下游抽）。
2. **`nondef_sucess_flip`（seed=79，反向翻转）**：缺省（21）**多**出成功臂 ⇒
   `RM_DELAYMAGIC`/`RM_POISON`/`SM_STRUCK` 多重集不等、`TargetHpBefore/TargetHp` 不等
   （`[{label}/...] TargetHp 不等：golden=100 rust=XX` 形态）。
3. **`nondef_hit`（seed=41，时长系数）**：`MabeTimeRate` 被硬编码时 `RM_POISON` 的时长参数不等
   （`Rm 多重集不等（...）`，`8037` 的对应字段）。

驱动侧（`fireball_case_assert`）先在本地拦住 `ServerConf` 不一致（存在性对称 + 绝对值），
消息形如 `[mabmabe-nondefault/nondef_hit] 台账 server_conf.mab_mabe_hit_rand_rate 必须等于部署值 50`；
**真正"吃缺省"的行为红仍由比较器行给出，两者互补，不许只留驱动侧**。

## 5. 前置缺陷：`EnsureConfFiles` 陈旧拷贝

见 [`openmir2-ensureconffiles-stale-copy.md`](openmir2-ensureconffiles-stale-copy.md)。
本卡落地时它必须是**独立第一笔**（否则两支可能建立在"静默沿用旧配置"上而看不出来）。

## 6. 成本与写集（问 4）

| 项 | 文件 | 规模 |
| --- | --- | --- |
| C# 装置 | `MagicGolden/ServerConfFixture.cs`（profile + 独立 sha + 十键期望） | ~40 行 |
| C# 装置 | `MagicGolden/Program.cs`（分支 + seed 扫描 + `EnsureConfFiles` 身份比较） | ~80 行 |
| Rust 驱动 | `bins/game-svr/src/lib.rs`（3 个 `#[ignore]` 驱动 + per-stem 期望表） | ~90 行 |
| Rust 比较器 | `tests/parity/tests/magic_fireball_parity.rs`（1 个 wrapper） | ~12 行 |
| 夹具 | `fixtures/serverconf/nondefault/{5 份 conf}` | 新增 5 文件 |
| golden | `golden/magic/mabmabe-nondefault.json` | 新增 1 文件 |
| **产品代码** | —— | **0 文件、0 调用点** |

比较器的绝对值表要 per-stem，并带**元断言**：被"第三值"覆盖的四键，其值 **≠ 部署值 && ≠ ctor 缺省**。

## 7. 落地顺序（等升格后）

1. 建分支（base 取**当时的 master**，不是 `0e2eb1316`）；
2. `EnsureConfFiles` 修复 + 三把牙（独立第一笔）；
3. 夹具目录（五份 conf，只改四键）+ sha 常量；
4. C# 分支 + seed 扫描 + §2.5 的 meta-assertion + §2.6 的两条 MC 变体断言；生成 golden
   （旧 stem 零变化用 base/new 快照复证）；
5. Rust 驱动 3 支 + 比较器 1 支；逐支 `--ignored --exact`，再跑比较器（同一 run_id）；
6. 变异牙：`apply_serverconf_fixture` 改成"只用缺省 100/10/21/20" ⇒ `nondef_randrate_flip`
   与 `nondef_sucess_flip` 必红、`nondef_hit` 必红；还原后 sha 逐字节相等；
7. 门禁：fmt / clippy / `cargo test -p game-svr --lib` / `serverconf_parse_parity`（须仍 2 passed）
   / `parity-gate -Mode Both`（红集合保持 `spacemove` ×3 + 1）。

## 8. 不采用的做法

- 直接改部署 `Server.conf`（会让 sha 钉死全部红、且污染生产配置）；
- 装载后在内存覆写 `SystemShare.Config`（绕过解析路径，自证）；
- 四键只改成"非部署但仍等于缺省"（等于没改）；
- 改 `mag_tamming_*` 当区分点（MG-48 不消费）。
