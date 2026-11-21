# 装置批次接线计划：ServerConf 配置装载（MG-44 圣言术 32 / MG-48 魔爆 50）

> **只读规划卡（docs-only）**：不动 `Program.cs` / `lib.rs`；实现另开卡。
> 基线：`master @ 66e26e4d7`（2026-11-21）。来源：`qa-next-batch-plan-20261121` 的 B 类共同前提。
>
> **前提更正（本卡实测）**：qa 计划里「harness 只载 `ExpConf` ⇒ 配置默认 0 ⇒ 分支恒不可达」**不成立**——
> `MagicGolden` 不载 `ServerConf` 是真的，但 `SystemShare.Config` 由 `SystemShare` 静态构造
> `new GameSvrConf()`，而 `GameSvrConf` 的**构造函数**逐项灌了缺省（见 §1/§2）。真正的问题是
> **缺省与部署值不一致**（`MagTurnUndeadLevel` 缺省 50 vs 部署 44），不是「0 ⇒ 不可达」。本批目标
> 应写成「**C# 从冻结 `Server.conf` 显式取值、Rust 从 golden 的 `server_conf` 取值**（同一生成源 + 钉 sha），
> fail-closed 断言到**具体期望值**」，而不是把 0 修成非 0。

## 1. oracle 依据

| 位置 | 内容 |
| --- | --- |
| `src/M2Server/Magic/MagicManager.cs:844` | `if (targetObject.Abil.Level < SystemShare.Config.MagTurnUndeadLevel)`——圣言术即死支的等级门（外层 `:842 Random(2)+(Level-1) > target.Level`、`:847 Random(100) < (magicLevel<<3)-magicLevel+15+level`） |
| `src/M2Server/Magic/MagicManager.cs:1243` | `Random(SystemShare.Config.MabMabeHitRandRate) < MAX(SystemShare.Config.MabMabeHitMinLvLimit, nLevel*8-nLevel+15+nLv)` |
| `src/M2Server/Magic/MagicManager.cs:1245` | `Random(SystemShare.Config.MabMabeHitSucessRate) < nLevel*2 + 4` |
| `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1231` 起的构造函数 | 各项缺省：`:1763 AmyOunsulPoint=10`、`:1773 MagTurnUndeadLevel=50`、`:1778 MabMabeHitRandRate=100`、`:1779 MabMabeHitMinLvLimit=10`、`:1780 MabMabeHitSucessRate=21`、`:1781 MabMabeHitMabeTimeRate=20` |
| `src/Modules/SystemModule/Conf/ServerConf.cs:530` / `:535`-`:538` / `:541` | `[Setup]` 键读取：`MagTurnUndeadLevel` / `MabMabeHitRandRate` / `MabMabeHitMinLvLimit` / `MabMabeHitSucessRate` / `MabMabeHitMabeTimeRate` / `AmyOunsulPoint`（缺键回落 `SystemShare.Config` 当前值＝ctor 缺省） |
| 部署 `Server.conf`（`E:/MirServer/M2GameSvr/Server.conf`，与 `E:/UserFleet/m1-stack/{configs/M2GameSvr,M2GameSvr}` 同值） | `AmyOunsulPoint=15`、`MagTurnUndeadLevel=44`、`MabMabeHitRandRate/MinLvLimit/SucessRate/MabeTimeRate = 100 / 10 / 21 / 20` |

**关键差异（同日两条）**：`MagTurnUndeadLevel` 部署 **44** ≠ C# ctor 缺省 50（Rust 缺省也是 50）；
`AmyOunsulPoint` 部署 **15** ≠ C# ctor 缺省 10（Rust `MabMabeConfig::default()` 也是 10）。
其余 `MabMabe*` 四项部署值＝缺省值（100/10/21/20）。

## 2. 装置现状（逐条可核）

- `MagicGolden` **只**调 `SystemModule.SystemShare.ExpConf.LoadConfig()`（`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`
  的 `CONF_EXPS_OK` 段），全文没有 `new ServerConf(...).LoadConfig()`；`ServerConfSha256()` 只算 sha 落 golden。
- 但 `src/Modules/SystemModule/SystemShare.cs:191` 的静态构造是 `Config = new GameSvrConf();`，而
  `GameSvrConf` 的构造函数（`:1231`）逐项写缺省 ⇒ `SystemShare.Config` **不是零初始化**：
  `MagTurnUndeadLevel=50`、`MabMabe* = 100/10/21/20`、`AmyOunsulPoint=10`。
- 结论：
  - **MG-44**：harness 用 50，部署用 44 ⇒ 即死支**可达**，但**阈值测错**（目标等级 45–49 时，
    golden 与生产行为相反）。这才是必须修的语义差异。
  - **MG-48**：harness 缺省 100/10/21/20 **等于**部署 ⇒ 今天可达且同值；但依赖 ctor 缺省是脆弱口径
    （缺省一改，golden 静默改变）。
  - 补充事实：C# `RandomNumber.Random(0)` = `random.Next(0)`、Rust `random_below(0)` = `next_max(0)`，
    两边都返回 0 且都**消耗一次采样**。所以即便某路径真以 0 运行：`:1243/:1245` 是 `0 < 正数`
    ⇒ **通过**（不是不可达）；会被 0 卡住的只有 `:844` 的 `Level < 0`。
- Rust 侧缺省：`World::new` 的 `mag_turn_undead_level` = 50；`World::new` 的 `b4_cfg` = `MabMabeConfig::default()`
  ＝ `hit_rand_rate 100 / hit_min_lv_limit 10 / hit_success_rate 21 / hit_mabe_time_rate 20 / amy_ounsul_point 10`；
  `game-svr` 的 `ServerConf` 解析缺键也回落 50 / 100 / 10 / 21 / 20 / 10。⇒ **两侧缺省彼此一致**，
  但与部署有**两处**不一致：`MagTurnUndeadLevel` 50 vs 44、`AmyOunsulPoint` 10 vs 15。
  施毒批内联只断 `AmyOunsulPoint <= 0`，**比不到 15**——共享入口必须断言 `== 15`。

## 3. 最小方案（共用装载入口，不造第二套）

已有先例：施毒批（WIP `card/poison-batch`，`git show 19aa909f9`）在其 `MagicGolden/Program.cs` 分支里
**内联**做了 `new SystemModule.Conf.ServerConf(<BaseDirectory>/Server.conf).LoadConfig()`，随后
`AmyOunsulPoint<=0 ⇒ FixtureContractViolation`；Rust 驱动侧从 golden `Fixture` 取 `amy_ounsul_point`
写 `rig.world.b4_cfg` 再 `set_b4_config`。本批把它**升级为一个共享入口**，两批共用：

1. **C# 侧**（新增文件，避免与施毒批在 `Program.cs` 里冲突）：
   `mir2-rs/tests/parity/csharp/MagicGolden/ServerConfFixture.cs`
   - `public static void LoadOrThrow(string caseName)`：定位 `<BaseDirectory>/Server.conf`，缺文件 ⇒
     `FixtureContractViolation`；`new SystemModule.Conf.ServerConf(path).LoadConfig()`；随后按调用方给的
     键清单断言**每个键等于该键的期望部署值**（`MagTurnUndeadLevel==44`、`AmyOunsulPoint==15`、
     `MabMabeHit*==100/10/21/20`；不等或 ≤0 即 `FixtureContractViolation`），**绝不吃缺省**——
     `>0` 不够，它放过 ctor 缺省 10/50。
   - 返回已装载值快照；落进 golden `Fixture` 的键名固定为（**ASCII 拼写跟 oracle 走**，
     `Sucess` 一个 c）：
     `server_conf: {mag_turn_undead_level, mab_mabe_hit_rand_rate, mab_mabe_hit_min_lv_limit,
     mab_mabe_hit_sucess_rate, mab_mabe_hit_mabe_time_rate, amy_ounsul_point,
     mag_tamming_level, mag_tamming_target_level, mag_tamming_hp_rate, mag_tamming_count}`。
     （注意 `MagTammingTargetHPRate` 是**键名**、C# 字段叫 `MagTammingHPRate`、golden 键叫
     `mag_tamming_hp_rate`——三者不同属 BUG-REPLICA，别混。）
   - `MG-44`（32）与 `MG-48`（50）分支在 `BehaviorBatch` 之前调用它；施毒批的 6/48 分支改为调用同一入口。
   - **边界**：`StringConf.LoadString()` + `YouPoisoned` 非空断言属于「字符串配置同源」，**不属于**本入口的
     十键职责——保留在施毒批自己的分支（或另立 `LoadStringOrThrow`）；圣言/魔爆/收服三批不为它多载字符串表。
   - **副作用说明（只写注释，不加代码）**：`ConfigFile.ReadWriteInteger`（`src/OpenMir2/Common/ConfigFile.cs:55-63`）
     对缺键会 `WriteInteger` 写回 **内存中的 ini 缓存**（不落磁盘）——加载有内存副作用，无需额外处理。
2. **Rust 侧**（parity 驱动区新增一个 helper）：
   `fn apply_serverconf_fixture(rig: &mut …, stem: &str, label: &str)`——从 golden 的 `Fixture` 读
   **必填**键：`mag_turn_undead_level` 落 `world.set_mag_turn_undead_level(v)`；`mab_mabe_hit_*` 落
   `world.b4_cfg`（`set_b4_config`）。缺键/≤0 ⇒ panic（契约违规）。
3. **同源证明（两条实链；Rust 侧不读 `Server.conf`，"两侧互比"是空转，不做）**：
   1) **生成端钉 sha**：`LoadOrThrow` 里 `FileSha256(path) != PINNED_SERVER_CONF_SHA256`（仓内常量）⇒
      `FixtureContractViolation`。当前生成源（`MAGICGOLDEN_CONF_DIR` 缺省 `E:\MirServer\M2GameSvr\Server.conf`）
      的 sha256 = `ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2`；该文件一改，golden
      生成当场红，而不是静默重基线。现有 `ServerConfSha256()` 已会算，缺的只是常量 + 断言。
      注：`E:/UserFleet` 两份部署副本是另一份文件（sha `f18068c6…`，其余键不同），本批十键值一致。
      **语义边界**：本包钉的 sha 是**生成源（`MAGICGOLDEN_CONF_DIR` 缺省 `E:\MirServer\M2GameSvr`）整文件身份**，
      用于「文件一改、golden 生成当场红」；它**不**用于证明所有部署副本同值（`E:/UserFleet` 两份 sha 不同、
      十键相同）。跨部署的一致性由 §3.3 第 2 条的**逐键字面值断言**承担。
      **不要绕过断言**：若将来把 `MAGICGOLDEN_CONF_DIR` 指向花名册副本，应**更新常量**，不得加 `--force` 旁路。
   2) **比较器断言绝对值**：对 golden 的 `server_conf` 逐键断言
      `mag_turn_undead_level == 44`、`mab_mabe_hit_rand_rate == 100`、`mab_mabe_hit_min_lv_limit == 10`、
      `mab_mabe_hit_sucess_rate == 21`、`mab_mabe_hit_mabe_time_rate == 20`、`amy_ounsul_point == 15`、
      `mag_tamming_level == 40`、`mag_tamming_target_level == 1`、`mag_tamming_hp_rate == 100`、
      `mag_tamming_count == 5` —— **不是"两侧相等"**。这是 magic 侧的唯一依赖（**不读文件**），
      保证 golden 仍是可移植的冻结件。
      **常量一致性**：十键常量在 C#/Rust 两侧各存一份（跨语言无法共享代码），但必须与
      `serverconf_parse_parity` 的 `DEPLOY` 数组**逐字一致**；实现批验收必须包含该测试仍
      `2 passed`（把两份常量钉在一起）。

**不要做全局装载**（不要在 `EnsureConfFiles` 后无条件 `LoadConfig()`）：那会一次性改变所有 stem 的
Config 面，必须全量重生成 + 全量对拍才能证明无漂移；**分支级装载**最小、可逐批验收。

## 4. fail-closed 口径（本仓惯例）

- 凡依赖配置项的批次：**缺文件 / 缺键 / 读值不等于期望部署值**（`MagTurnUndeadLevel=44`、`AmyOunsulPoint=15`、
  `MabMabe*=100/10/21/20`、`MagTammingLevel=40`、`MagTammingTargetLevel=1`、`MagTamming*HP/Count=100/5`）或 ≤0 ⇒ `FixtureContractViolation`（C#）或契约 panic（Rust），不许落缺省。
  **只断 `>0` 不够**：施毒批内联只断 `AmyOunsulPoint<=0`，比不到 15；本批共享入口断到具体值。
- golden 必须把**输入**写进 `Fixture`（逐键值 + `server_conf_sha256`），比较器只比"同一份输入的两侧输出"。

## 5. 风险与写集

- **既有 golden 影响**：`golden/magic/` 目前**没有** magic_id 32/50 的 stem（grep `"magic_id": 32|50` 0 命中），
  本批新增两份 JSON，不改既有文件。
- **若将来改成全局装载**：harness 还读 `Config.SnowWindRange / FireBoomRage / ElecBlizzardRange /
  MagicAttackRage / SkeletonCount / Dragon* / DragonCount / MaxSpellMsgCount`。当前部署值与 ctor 缺省
  **逐项相同**（1 / 1 / 2 / 8 / 1 / 神兽 / 1 / 1；部署 `Server.conf` 的这些键同值），所以今天"全局装载"
  大概率不改变既有 golden——但这不是契约，必须全量重跑证明；本卡不采用。
- **`MabMabe*` 0→非 0 的抽数风险**：缺省本身就是 100/10/21/20（非 0）且等于部署 ⇒ 装载**不改变**
  判定与抽数；即便如此，0 与非 0 的 `Random(n)` 都消耗一次采样（两侧对称），不存在"装载后多/少一抽"。
- **MG-44 阈值边界**：50→44 会改变 Level 45–49 不死目标的即死结果；golden 目标等级应覆盖
  **43 / 44 / 45** 三档，把 `Level < 44` 的两侧行为钉死。
- **写集（实现批）**：`MagicGolden/ServerConfFixture.cs`（新）、`MagicGolden/Program.cs`（两分支接入；
  **与施毒批协调**，建议先合施毒批再动）、`golden/magic/turnundead*.json`、`golden/magic/mabmabe*.json`、
  `bins/game-svr/src/lib.rs`（驱动 helper + 两处 fixture 应用）、`tests/parity/tests/magic_fireball_parity.rs`
  （比较器接入）；**另加独立对拍照**：`bins/game-svr/tests/serverconf_parse_parity.rs`（新）+
  `tests/parity/csharp/ServerConfProbe/`（新的小 C# 探针，只输出各键解析值）。**不碰 `src/**` oracle 代码。**

## 6. 验收与牙（建议，实现批落）

1. **装载失败红**：临时改名/删 `Server.conf`（夹具根）⇒ `FixtureContractViolation`，golden 不生成（rc 非 0）；
2. **键 ≤0 红**：把 `MagTurnUndeadLevel` 以 0 注入 ⇒ 同上（不许吃缺省）；
3. **阈值区分牙**：目标 `Level=43 / 44 / 45` 三档。**门序**（实测）：
   `:842 Random(2)+(caster.Level-1) > target.Level` → `:844` 等级门 → `:846` → 
   `:847 Random(100) < (magicLevel<<3)-magicLevel+15+level` → 即死。
   ⇒ **门外只有 `:842`；`:847` 在等级门之后（进了才抽）**。两处随机都由 golden 脚本钉死，但**钉的位置不同**
   （`:842` 是前置、`:847` 是有条件下游）。断言 **43 与 44 的即死结果不同**、**44 与 45 同侧**；
   依据严格小于：部署 44 ⇒ 43 过门、44 与 45 都不过门。**"必死"本身不可断言**（`:842` 前置随机 + `:847`
   下游随机各挡一次）。
4. **变异牙（真正的牙）**：把 Rust `mag_turn_undead_level` 从 golden 的 **44 改成 50** ⇒
   **44 与 45 两档必红、43 不红**（50 下 `44<50` 与 `45<50` 都成立、结果翻转；43 在两值下都过门，翻不了）。
   只盯 43 档抓不到 50。
5. **绝对值断言**：比较器对 `server_conf` 逐键断言 §3.3 第 2 条的十个字面值（不是"两侧相等"）。
6. **回归**：既有 31 份 magic golden 的 stem 全部不重新生成、不加 ServerConf 装载（git status 证）。
7. **独立的 config 解析对拍照（`3b` 方案）**：Rust 生产解析器（`bins/game-svr/src/config.rs` 的 `Server.conf`
   解析）与 C# `ServerConf.LoadConfig` 对**同一份文件**逐键相等；缺键时两侧回落值也相等（C# 回落 ctor 缺省、
   Rust 回落 `unwrap_or`）。**独立验收项**：不进 magic 驱动、不作为 golden 生成前提——否则所有 golden 都依赖
   外部 conf 路径存在，冻结件不再可移植。
