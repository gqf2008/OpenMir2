# 装置批次接线计划：MG-16 抗拒火环(8) / MG-57 气功波(37)

> 只读规划卡（docs-only）。**不动 `Program.cs` / `lib.rs`**；实现另开卡。
> oracle 里两支的函数体**逐字相同**（都 `MagPushArround(playObject, userMagic.Level)`，
> `result > 0 ⇒ boTrain = true`），所以可以合成**一个装置批次**、同一条驱动族。

## 1. oracle 依据

| 位置 | 内容 |
| --- | --- |
| `src/M2Server/Magic/MagicManager.cs:259-264` | `case SKILL_FIREWIND:` `if (MagPushArround(playObject, userMagic.Level) > 0) boTrain = true;` |
| `src/M2Server/Magic/MagicManager.cs:601-607` | `case SKILL_ENERGYREPULSOR:` **同形**（同 helper、同 `boTrain` 门；37 不在 `IsWarrSkill` 的 9 个魔号内） |
| `src/M2Server/Magic/MagicManager.cs:24-52` | `MagPushArround`：遍历 `playObject.VisibleActors` → 邻格门 `|Δx|<=1 && |Δy|<=1` → `!Death && != playObject` → `Level > target.Level && !StickMode` → `Random(20) < 6 + nPushLevel*3 + levelgap` → `IsProperTarget` → `push = 1 + MAX(0, nPushLevel-1) + Random(2)` → `GetNextDirection` → `target.CharPushed(nDir, push)` → `result++` |
| `src/M2Server/Actor/BaseObject.cs:854-901` | `CharPushed`：`Dir=nDir`、`nBackDir=GetBackDir(nDir)`，逐格 `GetFrontPosition → Envir.CanWalk(nx,ny,false) → Envir.MoveToMovingObject(...) → SendRefMsg(RM_PUSH, nBackDir, CurrX, CurrY, 0, "")`；`result==0 ⇒ Dir=olddir` |
| `src/M2Server/Player/PlayObject.Message.cs:1525-1535` | `RM_PUSH` ⇒ `SM_BACKSTEP(9)`；收件人含受推者自己 |

**Rust 侧现状（重要）**：产品侧**已接线**——`bins/game-svr/src/lib.rs` 的
`SKILL_FIREWIND | SKILL_ENERGYREPULSOR | SKILL_SPACEMOVE` 臂调 `world.cast_firewind_spell(...)`，
`bo_train = pushed > 0`，并按 `PushEvent` 逐格发 `SM_BACKSTEP`（`world.rs` 的 `cast_firewind_spell`
/ `push_now` / `char_pushed_core`）。**缺的不是实现，是 oracle 数据与装置能力**。

## 2. 装置缺什么（逐条可核）

### 2.1 `VisibleActors` 恒空 ⇒ `MagPushArround` 循环零次
- 依据：`MagPushArround` 遍历 `playObject.VisibleActors`（`src/M2Server/Magic/MagicManager.cs:27`）。
- 现状：`MagicGolden` 装置里 **没有任何 `UpdateVisibleGay` 调用**（`grep UpdateVisibleGay` 0 命中），
  对象只经 `env.AddMapObject(...)` 入图。⇒ 即使目标在邻格，循环体一次都不进。
- **最小回填**：对每个"应当可见"的目标调用 `caster.UpdateVisibleGay(target)`
  （`PlayObject.cs:3042` / `BaseObject.ViewRange.cs:12`：不存在则 `VisibleActors.Add(VisibleFlag.Show)`；
  已存在则置 `Invisible` 并返回）。注意 `MagPushArround` **不看 `VisibleFlag`**，只遍历列表。
- 反向（`target.UpdateVisibleGay(caster)`）只在需要"目标自己也有视野"的支才要；本批**不需要**。

### 2.2 `CharPushed` 要可走地图 ⇒ 需要 `CanWalk`/`MoveToMovingObject` 真语义
- 现状：装置已 `env.LoadMapData(0.map)`（`MagicGolden/Program.cs` 的 `RunFireball` 里 `env.LoadMapData(0.map)`；`MAGICGOLDEN_MAP_DIR` 缺省
  `E:\MirServer\M2GameSvr\Map`），并把 caster/target 用 `AddMapObject` 入图 ⇒ **地图是真 Envirnoment**，
  `OfflineMapSystem` 只替身 `IMapSystem`（9 个成员未用），不挡 `Envir.CanWalk`。
- **测量（一次性；不开图、不进 golden）**：实测 caster/target 落点片及其推挤方向的邻格 `cellSuccess`
  ——装置里 `Console.Error.WriteLine` 打 `env.CanWalk(x,y,false)`，范围 `x∈[97..104], y∈[99..101]`；
  **结果要写回本卡**（"已实测：`(x,y)` 可走 / 不可走"）。**不可走就换一组落点再测，不靠碰运气。**
- **与 `3b` 审计的分工（两件事，别混）**：`3b` 证的是"`VisibleActors` 表空 ⇒ `MagPushArround` 循环零次
  ⇒ `CanWalk` **根本不会被调用**"；这**不等于**"回填视野后能推进格"。前者已证，后者**尚未有人实测**
  ⇒ 本条的测量仍然必须做（回填之后 `CharPushed` 真的会走到 `CanWalk`）。
- Rust 侧对应：`char_pushed_core` 在 `sim.maps.get(map_id)` 为 `None` 时**直接返回空事件**
  ⇒ rig 必须给该 actor 的地图；否则"目标不动"会被误读成"Rust 推不动"。

### 2.3 其它可设项（已具备，确认即可）
- `Abil.Level`：`MagicGolden/Program.cs` 的 `RunFireball` 已设（30/20）⇒ 等级门可通过；
- `StickMode`：需要一支**负例**（StickMode=true ⇒ 零抽、零帧）；
- `IsProperTarget`：装置里玩家无组队 ⇒ 恒 proper（`cast_firewind_spell` 用
  `combat::is_proper_target_grouped`，同源）。

## 3. 断言面

以"caster(100,100) L30 / target(101,100) L20 / obs_in(105,100)"为例（`nPushLevel = userMagic.Level = 1`）：

| 收件人 | 期望帧 |
| --- | --- |
| caster | `17`（`SM_SPELL`，`DoSpell` 开头无条件广播）、`638`（`SM_MAGICFIRE`，共享尾）、`640`（`SM_MAGIC_LVEXP`，1000ms，仅当 `boTrain`） |
| target（被推） | `17`、`638`、**`SM_BACKSTEP(9)` × 实际推进格数**（逐格一条，保序；``Recog/RM` 口径按 `hbfireball.json` 的既有样例） |
| obs_in | `17`、`638`（±range 内）；若不在被推者广播范围则**没有** 9 |
| obs_out | **零帧** |

**抽数账本**：`Random(20)` **每个通过"邻格+等级+StickMode"门的目标各一次**；
`Random(2)` **每次成功调用 `CharPushed` 一次**（不看实际推进格数）。
即：一个目标命中 ⇒ 恰 2 抽 + 逐格 `MoveToMovingObject` 不抽。

### 3.1 退化守卫（验收硬条件；8/37 与 MG-47 同款）

**golden 里必须存在 ≥1 条推挤帧**（两支都是 `SM_BACKSTEP(9)`）；**零条即具名红**：
断言文案要写清"本 case 未产生任何推挤帧 ⇒ 效果体零次执行"，**不许静默通过**。

理由：`17`/`638`/`640` 都在、帧集合"看起来对"，但推挤体一次都没执行——这是最难发现的假绿。
同款教训见 `docs/issues/openmir2-groupamyounsul-nondegenerate-golden.md`（四刀变异全绿 = 循环零次）。
本条也是 §4 牙④（"视野为空"）的**产品化**：牙④ 证明守卫有牙，本守卫保证**合入后**不会退回退化态。

## 4. 牙（每颗写清红在哪）

| # | 变异 | 期望红在 |
| --- | --- | --- |
| ① | 去掉 `Random(20)` 门（恒通过） | 抽数账本（多抽）+ 可能的 `9` 帧；红在 RNG 断言 |
| ② | 去掉邻格门 `|Δx|<=1 && |Δy|<=1` | 远处目标（obs_in）也进循环 ⇒ 多抽 + 多帧 |
| ③ | `CharPushed` 换无操作（`push_now` 直接返回空） | **`9` 帧消失**但 `boTrain`/`640` 仍在 ⇒ 红在帧集合 |
| ④ | **视野为空**（不调 `UpdateVisibleGay`） | 零抽、零 `9` 帧、`boTrain=false`（→ 无 `640`）⇒ 红在帧集合与抽数 |

④ 是**退化夹具的守卫**——本批的价值一半在这里（对照 `MG-50` 的教训：接线验通 ≠ 效果体验通）。

## 5. 写集与风险

| 文件 | 改动 |
| --- | --- |
| `mir2-rs/tests/parity/csharp/MagicGolden/Program.cs` | 新 case 入口：造目标 + **`UpdateVisibleGay` 回填** + `StickMode/Level` 设置 + 帧/RM/RNG 采集 |
| `mir2-rs/tests/parity/csharp/MagicGolden/FixtureContract.cs`（可能） | 新 fixture 键（`push_level` / `stick_mode` / 目标坐标） |
| `mir2-rs/tests/parity/golden/magic/{firewind,energyrepulsor}.json`（新） | oracle 数据（本批新造，属"新 oracle 数据"，需按批次流程采集） |
| `mir2-rs/bins/game-svr/src/lib.rs` | 驱动 `firewind_parity_*` / `energyrepulsor_parity_*`（复用 `fireball_parity_drive`） |
| `mir2-rs/tests/parity/tests/magic_fireball_parity.rs` | `real_rust_ledger_{firewind,energyrepulsor}` |

**风险等级：B（共享装置）**。理由：`Program.cs` 是所有 magic stem 共用的装置；`UpdateVisibleGay`
回填若被误加在公共路径上，会改变**其它 stem** 的 `VisibleActors`（进而是 `SendRefMsg` 收件人集合）
⇒ **必须只在 8/37 的新 spec 分支里做**，并在合入前跑**全部既有 magic 比较器**（当前 21+ 支）确认零漂移。

**在飞冲突面（截至本卡）**：
- `card/mg18-fix`（魔号 10）与 `card/poison-batch` 都动 `MagicGolden/Program.cs` 与 `bins/game-svr/src/lib.rs`
  ⇒ 本批实现时**先 rebase 到当时 master**，冲突处按 oracle 解、不取一侧；
- 本批不碰 `handle_spell_cmd_with_magic` 的既有效果体（8/37 臂已存在），只在**装置与测试**层加东西，
  产品代码预期**零改动**——若实现时发现产品侧需要改，先停下来报。
