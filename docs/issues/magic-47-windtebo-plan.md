# MG-47 / 魔号 35 `SKILL_WINDTEBO`（狂风·灭天火）接线计划

> 只读规划卡（docs-only）。**不写实现、不改 `lib.rs`/`magic.rs`**（施毒批在改）。
> 清单状态：**`missing`（未实现）** —— Rust 侧既无常量也无效果体（见 Q1 末）。
> 与 `MG-16/MG-57`（8/37 推挤批）共用 `CharPushed` 产品链，但**目标选择与骰式不同**（见 Q2）。

## Q1 `GetPoseCreate()` 的语义 + Rust 对应件

**oracle**：`src/M2Server/Actor/BaseObject.cs:1253-1262`

```csharp
public IActor GetPoseCreate() {
    short nX = 0, nY = 0;
    if (GetFrontPosition(ref nX, ref nY)) return Envir.GetMovingObject(nX, nY, true);
    return null;
}
```

- **取"面朝方向的邻位实体"，不是当前攻击目标**：`GetFrontPosition`（`BaseObject.cs:977-1040`）
  按 `Dir` 取前一格，**越界时 clamp（原样返回当前格）**；
  `Envirnoment.GetMovingObject(x, y, true)`（`src/M2Server/Maps/Envirnoment.cs:1203-1232`）
  在该格 `ObjList` 里取**首个 `!Ghost` 且（`boFlag=true` ⇒）`!Death`** 的 actor。
- 与"目标选择"无关：`MagWindTebo` 完全不看 `DoSpell` 的 `targetObject`。

**Rust 侧**：
- 有 `front_position`（`mir2-rs/crates/world-bevy/src/magic.rs:802`，逐行移植 `GetFrontPosition`，
  越界 clamp）；
- 有 `World::moving_object_at_impl`（`mir2-rs/crates/world-bevy/src/world.rs:6558`，
  `boFlag=true` 语义已在 `monai` 多处使用，例：`mir2-rs/crates/world-bevy/src/monai.rs:4098`）；
- **没有** `pose_create` 这个命名封装 ⇒ 实现时按 `front_position` + `moving_object_at_impl(map, fx, fy, true)`
  两步组合（与 `monai` 的既有写法一致），**不需要新建抽象**。
- **`SKILL_WINDTEBO` 常量本身不存在**（全仓 grep 无命中）；`magic_id = 35`（`src/Modules/SystemModule/Const/MagicConst.cs:142`）。

## Q2 与 8/37 `MagPushArround` 的共用面

| 面 | 8/37 `MagPushArround`（`MagicManager.cs:24-52`） | 35 `MagWindTebo`（`:858-874`） | 一致？ |
| --- | --- | --- | --- |
| 推挤动作 | `target.CharPushed(nDir, push)`（`:43`） | `target.CharPushed(dir, (byte)(MAX(0,Level-1)+1))`（`:871`） | **同一方法** `BaseObject.CharPushed`（`BaseObject.cs:854-901`）⇒ 逐格 `CanWalk`/`MoveToMovingObject`/`RM_PUSH` **完全一致** |
| 帧面 | 逐格 `RM_PUSH` ⇒ `SM_BACKSTEP(9)` | 同左 | ✅ 一致 |
| 目标来源 | 遍历 `VisibleActors`（`:27`） | `GetPoseCreate()`（**前格单个**，`:861`） | ❌ 不同 |
| 骰式 | `Random(20) < 6 + L*3 + gap`（`:37`） | `Random(20) < L*6 + 6 + gap`（`:869`） | ❌ 系数不同（3 vs 6） |
| 推挤格数 | `1 + MAX(0,L-1) + Random(2)`（`:41`） | `MAX(0,L-1) + 1`（**无 `Random(2)`**，`:871`） | ❌ 本支**确定性** |
| 抽数 | 每合格目标 1 次 `Random(20)`；每次成功 1 次 `Random(2)` | **至多 1 次 `Random(20)`**，无 `Random(2)` | 不同 |

⇒ **产品链（`CharPushed`）完全共用，调用方三处不同**。因此：
- **必须同批做**：同一套装置能力（可走地图、`boTrain`→640、逐格 `SM_BACKSTEP` 断言）只建一次；
  且 `Character` 侧推挤语义若有偏差，两批会同时暴露——分开做等于把同一处验证做两遍、且第一遍的结论会被第二遍推翻；
- **不能复用 `push_chance`**（`mir2-rs/crates/world-bevy/src/magic_effect.rs:42`，系数 3）与
  `push_steps`（`magic_effect.rs:48`，含 `Random(2)`）：本支需要**自己的**两个站点
  （`windtebo_chance`：`random_below(20) < level*6 + 6 + gap`；格数 = `1 + max(0, level-1)` 无抽）。

## Q3 三个守卫 + `Death` 的 Rust 等价物

| 守卫 | oracle | Rust |
| --- | --- | --- |
| `StickMode` | `!targetObject.StickMode`（`MagicManager.cs:862`） | `Entity::stick_mode`（`mir2-rs/crates/world-bevy/src/entity.rs:611`；只读镜像 `EntityRef` 同名字段 `:1158`） |
| `Ghost` | `!targetObject.Ghost`（`:862`） | `Entity::ghost`（`entity.rs:613`） |
| `IsProperTarget` | `playObject.IsProperTarget(targetObject)`（`:862`） | `combat::is_proper_target_grouped`（`mir2-rs/crates/world-bevy/src/combat.rs:721`；直接版 `_direct` 在 `:746`） |
| `Death` | `!targetObject.Death`（`:862`） | `Entity::death`（`entity.rs:612`） |

（与 `cast_firewind_spell` 用的是同一批字段；`GetPoseCreate` 自身已按 `boFlag=true` 滤掉 `Ghost`/`Death`，
守卫链里的这两条是**冗余但必须保留**——决定"是否抽 `Random(20)`"。）

## Q4 RNG 账本（本支抽数 与 共享尾抽数 分开）

| 段 | 抽 | 条件 | 站点 |
| --- | --- | --- | --- |
| **本支** | `Random(20)` **1 次** | 守卫链全过（前格有实体、非自己、`!Death`、`!Ghost`、proper、`!StickMode`、邻格、等级） | oracle `:869`；Rust 新增站点（不复用 `push_chance`） |
| **`CharPushed` 内部** | **0 次** | — | `BaseObject.cs:854-901` 无 RNG |
| **共享尾** | `Random(3)` **1 次** | `userMagic.Level < 3 && boTrain` 且 `TrainLevel[Level] <= Abil.Level` | oracle `MagicManager.cs:662-668`；Rust `magic::check_skill_proficiency`（`mir2-rs/crates/world-bevy/src/magic.rs:525`）+ 调用方 `Random(3)+1` |

**帧**：`17`（`SM_SPELL`，`DoSpell` 开头无条件）+ `638`（`RM_MAGICFIRE`，`:651-658`，`p3 = targetObject?.ActorId ?? 0`
——**是 DoSpell 的目标**，不是 `GetPoseCreate` 的）+ 逐格 `9`；`boTrain` 且训练门过 ⇒ `640`（1000ms 延迟）。
**不要把"本支 1 抽"和"尾 1 抽"写成同一个数**：前者恒定≤1，后者随 `level/TrainLevel/boTrain` 变化。

## Q5 装置需要什么（对照 8/37 卡）

| 需求 | 8/37 卡（`magic-push-batch-wiring-plan.md`） | 本支 |
| --- | --- | --- |
| 视野回填 `UpdateVisibleGay` | **必需**（`MagPushArround` 遍历 `VisibleActors`） | **不需要**（`GetPoseCreate` 走地图格，不看视野表） |
| 可走地图 / `CanWalk` | 必需（`(100,100)` 一带 `cellSuccess` 待探） | **同一条**，直接继承 |
| 前格构造 | 不需要 | **新增**：目标必须 `AddMapObject` 到 **caster.Dir 指向的那一格**。装置**已能设 `Dir`**：`Program.cs:1998` 在 `spec.EnableMaps` 时 `caster.Dir = GetNextDirection(caster, doX, doY)`（`GetFrontPosition` 用 `Dir`；注释见 `Program.cs:1996`）⇒ 复用该机制，**不需要新开关** |
| 负例构造 | 邻格/等级/视野 | 前格为空、前格是 `Ghost`/`Death`、`StickMode=true`、等级不低于、非 proper |

## 牙（每颗写清红在哪）

| # | 变异 | 期望红 |
| --- | --- | --- |
| ① | 把 `Random(20) < L*6+6+gap` 换成 8/37 的 `L*3` 系数 | `Random(20)` 的**通过率**变化 ⇒ 抽数账本或帧集合红（需选能区分的种子，卡记两档种子） |
| ② | 推挤格数改成 `+Random(2)`（误抄 8/37） | 多抽 1 次 ⇒ **RNG 账本红**（帧可能相同，抽数不同） |
| ③ | `GetPoseCreate` 换成"遍历 `VisibleActors`" | 前格以外的目标也被推 ⇒ 帧集合红 |
| ④ | 去掉 `StickMode` 门 | 该目标仍零帧、但**多抽 1 次 `Random(20)`** ⇒ 抽数红 |
| ⑤ | 前格为空（不把目标放进前格） | 零抽零帧、`boTrain=false`（无 640）⇒ 退化守卫 |

## 写集与风险

| 文件 | 改动 |
| --- | --- |
| `mir2-rs/crates/world-bevy/src/world.rs` | 新增 `cast_windtebo_spell`（`GetPoseCreate` + 守卫 + 骰 + `push_now`） |
| `mir2-rs/crates/world-bevy/src/magic_effect.rs` | 新增 `windtebo_chance` 站点（**不复用** `push_chance`） |
| `mir2-rs/crates/world-bevy/src/world.rs`（常量表） | 新增 `SKILL_WINDTEBO = 35` |
| `mir2-rs/bins/game-svr/src/lib.rs` | host 臂（`35 =>`）+ 驱动 `windtebo_parity_*` |
| `mir2-rs/tests/parity/csharp/MagicGolden/Program.cs` | 新 spec：前格构造 + `caster.Dir` + 采集 |
| `mir2-rs/tests/parity/golden/magic/windtebo.json`（新） | 新 oracle 数据 |
| `mir2-rs/tests/parity/tests/magic_fireball_parity.rs` | `real_rust_ledger_windtebo` |

**风险：B 类（共享装置 + 新增产品代码）**——与 8/37 批不同的是本支**确实要写产品代码**
（`cast_windtebo_spell` + 常量 + host 臂），因此：
1. 必须先与 8/37 批的 `CharPushed` 语义对齐（同批做，见 Q2）；
2. 产品改动会进入 `bins/game-svr` 与 `crates/world-bevy`，**不是** 8/37 那种"零产品改动"；
3. 落 `Program.cs` 时与 `card/mg18-fix`、`card/poison-batch` 的冲突面同 8/37 批；
4. 合入前须跑**全部既有 magic 比较器零漂移**（新增常量/臂不得改变既有魔号的行为）。

## 边界与未决

- 本卡不写实现、不动 `lib.rs`/`magic.rs`；实现另开卡。
- 未决（需产品确认）：35 是否在客户端魔表里有独立的 `TrainLevel`/`EffectType` 行（决定 640 与 638 的
  `param` 取值）——本卡未在 `magic` 表数据里核实，实现时以 `sql/` 冻结数据为准。
