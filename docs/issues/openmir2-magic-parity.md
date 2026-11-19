# openmir2-magic-parity

> 状态：**进行中（2026-11-19，分支 `card/magic-parity`，base M9=`572a0b8a3`）**。
> Owner 指示：性能线冻结，全力补玩法对齐；魔法是四块（技能/物品/任务/NPC 脚本）里先补的一块，
> 且**必须按 ECS 实现**；对外的覆盖率口径是 `verified/65`（未验证的实现在覆盖率上等于 0）。

## 1. 覆盖率口径与清单

- 分母：`src/M2Server/Magic/MagicManager.cs` 的 `case` 分支数 = **65**（61 个 `MagicConst` case
  + 4 个 `Shape` 子 case；含 `IsWarrSkill` 的 9 个 case）。
- 清单文件：`E:\UserFleet\magic-parity.json`（每 15 分钟由 `E:\OPC\tools\coverage-report.ps1` 读取）。
  首次产出：**verified 0 / implemented 63 / missing 2**。
- `verified` 的门槛：**与原版同输入、帧级对拍通过**（帧逐字节 + RNG 抽取次数与逐次取值全等），
  并在清单里写测试名。没有测试名不许标 `verified`。
- `missing` 2 条：35 `WINDTEBO`（`MagicManager.cs:524-529`）、
  39 `GROUPDEDING`（`MagicManager.cs:549-553`）。
- 单独待办（不混进 implemented）：
  1. `buff_tick_gate` — 14/15/31 的 buff 生产链仅 `World::buff_tick_enabled()` 时生效（缺省关），
     `mir2-rs/bins/game-svr/src/lib.rs:13229`；
  2. `anti_magic_recalc` — 目标 `AntiMagic` 的 `RecalcAbilitys` 面未同步（`lib.rs:12170`）；
  3. `version_gate_167` — `DoSpell` 版本门未接（`MagicManager.cs:167`）。

## 2. 对拍装置（先建装置，再迁移/补分支）

**同输入**：`(magic_id, magic_level, 施法者 MC/SC/DC/HitPoint、目标 AntiMagic/SpeedPoint/LifeAttrib/Race、
双方坐标、地图遮挡、随机源序列、配置项)`。

**C# 侧**：`mir2-rs/tests/parity/csharp/MagicGolden/`（新工程，引用 `src/M2Server/M2Server.csproj`
与 `src/OpenMir2/OpenMir2.csproj`），调用**原版** `MagicManager.DoSpell`，落盘
`frames[]`（出帧顺序与逐字节）与 `rng[]`（每次抽取的值，按调用序）。

**RNG 替换点（不改进生产代码）**：`OpenMir2.RandomNumber` 的**私有静态字段 `random`**
（`src/OpenMir2/RandomNumber.cs:11`）。driver 先 `RandomNumber.GetInstance()` 初始化单例，
再用反射把该字段替换为 `LoggingRandom : Random`（固定种子；覆写 `Next()/Next(int)/Next(int,int)`，
逐次记录到账本）。

**还原方式**：替换是**进程内**的，driver 跑完即退出、进程销毁即还原；若同进程跑多用例，
每例前重新设种子、例后把原 `Random` 实例写回并断言 `GetInstance()` 返回同一单例。
**生产代码零改动**：没有 `#if`、没有测试专用分支、没有第二份 `MagicManager`。

**帧捕获点（下一步要解决的第一个技术问题）**：`DoSpell` 的出帧走
`playObject.SendRefMsg` / `SendSelfDelayMsg`（及 `MagMakeFireCross` 等的 `SendRefMsg`），
需要确认这些函数在**无会话**的离线对象上能否被拦截/记录（候选：`BaseObject` 的发送队列，
或给 `PlayObject` 挂一个只记录不发送的会话桩），确定后写进本节。

**判据**：帧逐字节相等 **且** RNG 抽取次数与逐次取值全等 ⇒ 该分支在清单里标 `verified` 并写测试名。

## 3. 批序（先高频且确定）

1. `1/5 火球`（`MagicManager.cs:174-203`）
2. `11/45 雷电`（`:289-314` / `:563-588`）
3. `2 治愈`（`:204-224`）
4. `13 灵魂火符`（`:315-327`、`:328-351`）
5. `44 灭天火`（`:557-562`）
6. 再处理 buff / AntiMagic 复算相关（依赖第 1 节的三条待办先修）

每验证一条：`magic-parity.json` 对应条目改 `verified` + 写测试名；跑
`powershell -NoProfile -ExecutionPolicy Bypass -File E:\OPC\tools\coverage-report.ps1` 自检。

## 4. ECS 迁移清单（Owner 硬约束：状态组件化 + 行为系统化 + 段序钉死）

**迁移对象（逐条给 `文件:行`，代码现状为准）**

| # | 分支（原版） | Rust 现有入口 | oracle 依据 | 迁移顺序 |
|---|---|---|---|---|
| 1 | 21 瞬息移动 | `crates/world-bevy/src/magic.rs:904` `mag_sace_move`；`crates/world-bevy/src/world.rs:7070` `cast_space_move_spell`；host `bins/game-svr/src/lib.rs:13527` | `MagicManager.cs:403-410`、`:876-891` | 1 |
| 2 | 31 魔法盾 | `crates/world-bevy/src/world.rs:4395` `cast_shield_spell`；host `lib.rs:13306` | `MagicManager.cs:474-478` | 2 |
| 3 | 22 火墙 | `crates/world-bevy/src/world.rs:751` 返回值；`crates/world-bevy/src/magic.rs:188` `power_fire_wall`；host `lib.rs:15300` | `MagicManager.cs:412-413`、`:1064-1098` | 3 |
| 4 | 8/37 推人 | `crates/world-bevy/src/magic.rs:826` `mag_push_arround`；host `lib.rs:13552`、`lib.rs:68031` | `MagicManager.cs:24-52`、`:259-264`、`:601-606` | 4 |
| 5 | 46 分身 | `crates/world-bevy/src/magic.rs:967` `mag_make_clone`；`crates/world-bevy/src/slave.rs:2534` | `MagicManager.cs:589-594`、`:1337-1341` | 5 |
| 6 | 20 驯服 | `crates/world-bevy/src/magic_effect.rs`（`mag_tamming`，经 `magic.rs:35` 再导出） | `MagicManager.cs:394-401`、`:704-825` | 6 |
| 7 | 28 心灵启示 | `crates/world-bevy/src/world.rs:5801` `cast_show_hp_spell` | `MagicManager.cs:431-440` | 7 |

**要求（每个迁移必须同时给）**
1. **状态组件化**：冷却/技能等级/施法目标/吟唱进度做成 Component，走 `Query`/`ResMut`；
2. **行为系统化**：施法判定、MP 扣除、命中/伤害结算、特效广播各自成 system，注册进 `HeadRunner`
   的 `TickStage` 序列（`crates/world-bevy/src/world.rs:1838-1856` 是现有序列）；
3. **段序写死并测试钉住**：卡面写明"在 head 还是 tail、在 AOI 前后"及理由；新增顺序测试；
4. **行为不变**：迁移提交必须附该分支的对拍证据（第 2 节装置）；
5. **确定性**：系统顺序固定 ⇒ RNG 消耗顺序固定。

**禁止**：为快继续加自由函数；绕过 tick 阶段在主循环里插代码。

## 5. 边界

- 不碰 AOI、网关/背压、性能（性能线已按 Owner 指示冻结）；
- 不改 C# 原版逻辑（RNG 替换只在测试进程内，见第 2 节）；
- 每批走 `card/magic-*` 分支 + `--no-ff`，小步提交，贴对拍原文。
