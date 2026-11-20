# 装置批次接线计划：ServerConf 配置装载（MG-44 圣言术 32 / MG-48 魔爆 50）

> **只读规划卡（docs-only）**：不动 `Program.cs` / `lib.rs`；实现另开卡。
> 基线：`master @ 66e26e4d7`（2026-11-21）。来源：`qa-next-batch-plan-20261121` 的 B 类共同前提。
>
> **前提更正（本卡实测）**：qa 计划里「harness 只载 `ExpConf` ⇒ 配置默认 0 ⇒ 分支恒不可达」**不成立**——
> `MagicGolden` 不载 `ServerConf` 是真的，但 `SystemShare.Config` 由 `SystemShare` 静态构造
> `new GameSvrConf()`，而 `GameSvrConf` 的**构造函数**逐项灌了缺省（见 §1/§2）。真正的问题是
> **缺省与部署值不一致**（`MagTurnUndeadLevel` 缺省 50 vs 部署 44），不是「0 ⇒ 不可达」。本批目标
> 应写成「**两侧都显式、fail-closed 地从冻结 `Server.conf` 取值**」，而不是把 0 修成非 0。

## 1. oracle 依据

| 位置 | 内容 |
| --- | --- |
| `src/M2Server/Magic/MagicManager.cs:844` | `if (targetObject.Abil.Level < SystemShare.Config.MagTurnUndeadLevel)`——圣言术即死支的等级门（外层 `:842 Random(2)+(Level-1) > target.Level`、`:847 Random(100) < (magicLevel<<3)-magicLevel+15+level`） |
| `src/M2Server/Magic/MagicManager.cs:1243` | `Random(SystemShare.Config.MabMabeHitRandRate) < MAX(SystemShare.Config.MabMabeHitMinLvLimit, nLevel*8-nLevel+15+nLv)` |
| `src/M2Server/Magic/MagicManager.cs:1245` | `Random(SystemShare.Config.MabMabeHitSucessRate) < nLevel*2 + 4` |
| `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1231` 起的构造函数 | 各项缺省：`:1763 AmyOunsulPoint=10`、`:1773 MagTurnUndeadLevel=50`、`:1778 MabMabeHitRandRate=100`、`:1779 MabMabeHitMinLvLimit=10`、`:1780 MabMabeHitSucessRate=21`、`:1781 MabMabeHitMabeTimeRate=20` |
| `src/Modules/SystemModule/Conf/ServerConf.cs:530` / `:535`-`:538` / `:541` | `[Setup]` 键读取：`MagTurnUndeadLevel` / `MabMabeHitRandRate` / `MabMabeHitMinLvLimit` / `MabMabeHitSucessRate` / `MabMabeHitMabeTimeRate` / `AmyOunsulPoint`（缺键回落 `SystemShare.Config` 当前值＝ctor 缺省） |
| 部署 `Server.conf`（`E:/MirServer/M2GameSvr/Server.conf`，与 `E:/UserFleet/m1-stack/{configs/M2GameSvr,M2GameSvr}` 同值） | `AmyOunsulPoint=15`、`MagTurnUndeadLevel=44`、`MabMabeHitRandRate/MinLvLimit/SucessRate/MabeTimeRate = 100 / 10 / 21 / 20` |

**关键差异**：`MagTurnUndeadLevel` 部署 44 ≠ 缺省 50（其余四项部署值＝缺省值）。

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
  但都与部署的 `MagTurnUndeadLevel=44` 不一致。

## 3. 最小方案（共用装载入口，不造第二套）

已有先例：施毒批（WIP `card/poison-batch`，`git show 19aa909f9`）在其 `MagicGolden/Program.cs` 分支里
**内联**做了 `new SystemModule.Conf.ServerConf(<BaseDirectory>/Server.conf).LoadConfig()`，随后
`AmyOunsulPoint<=0 ⇒ FixtureContractViolation`；Rust 驱动侧从 golden `Fixture` 取 `amy_ounsul_point`
写 `rig.world.b4_cfg` 再 `set_b4_config`。本批把它**升级为一个共享入口**，两批共用：

1. **C# 侧**（新增文件，避免与施毒批在 `Program.cs` 里冲突）：
   `mir2-rs/tests/parity/csharp/MagicGolden/ServerConfFixture.cs`
   - `public static void LoadOrThrow(string caseName)`：定位 `<BaseDirectory>/Server.conf`，缺文件 ⇒
     `FixtureContractViolation`；`new SystemModule.Conf.ServerConf(path).LoadConfig()`；随后按调用方给的
     键清单断言**每个键 > 0**（失败抛 `FixtureContractViolation`，绝不吃缺省）。
   - 返回已装载值快照（供 golden `Fixture` 落逐键值与 `server_conf_sha256`）。
   - `MG-44`（32）与 `MG-48`（50）分支在 `BehaviorBatch` 之前调用它；施毒批的 6/48 分支改为调用同一入口。
2. **Rust 侧**（parity 驱动区新增一个 helper）：
   `fn apply_serverconf_fixture(rig: &mut …, stem: &str, label: &str)`——从 golden 的 `Fixture` 读
   **必填**键：`mag_turn_undead_level` 落 `world.set_mag_turn_undead_level(v)`；`mab_mabe_hit_*` 落
   `world.b4_cfg`（`set_b4_config`）。缺键/≤0 ⇒ panic（契约违规）。
3. **同源证明**：golden `Fixture` 已含 `server_conf_sha256`（`Program.cs` 输出）；驱动侧把它与
   运行时读到/冻结的 sha 比对，不等 ⇒ 契约违规。

**不要做全局装载**（不要在 `EnsureConfFiles` 后无条件 `LoadConfig()`）：那会一次性改变所有 stem 的
Config 面，必须全量重生成 + 全量对拍才能证明无漂移；**分支级装载**最小、可逐批验收。

## 4. fail-closed 口径（本仓惯例）

- 凡依赖配置项的批次：**缺文件/缺键/读值 ≤ 0 ⇒ `FixtureContractViolation`**（C#）或契约 panic（Rust），
  不许落缺省。理由：缺省是"源码默认"，部署是另一份真值，二者任一漂移都应让 golden 生成失败而不是静默变绿。
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
  （比较器接入）。**不碰 `src/**` oracle 代码。**

## 6. 验收与牙（建议，实现批落）

1. **装载失败红**：临时改名/删 `Server.conf`（夹具根）⇒ `FixtureContractViolation`，golden 不生成（rc 非 0）；
2. **键 ≤0 红**：把 `MagTurnUndeadLevel` 以 0 注入 ⇒ 同上（不许吃缺省）；
3. **阈值牙**：目标 `Level=44` 必死、`Level=45` 必不死（部署值 44；断言写死）；
4. **两侧同值**：golden `server_conf_sha256` 与运行侧读到的 sha 相等，且逐键值相等；
5. **回归**：既有 31 份 magic golden 的 stem 全部不重新生成、不加 ServerConf 装载（git status 证）。
