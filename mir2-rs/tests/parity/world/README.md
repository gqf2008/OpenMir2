# M2 世界/AOI 对拍（同地图同实体 → 位置与可见集逐项相同）

对应设计文档 §6 M2 判据②与 §4.2（世界模拟单线程串行）。夹具与金标准生成器在
`tests/parity/csharp/WorldGolden/`，场景在 `world/scenarios/`，金标准在 `world/golden/`。

## 一条命令

```bash
# 1) C# oracle 产出金标准（函数体逐字拷贝；路径自解析，不依赖 cwd）
dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- basic

# 2) Rust 侧对拍（位置 + 可见集，逐行）
cargo test -p mir2-parity-tests --test world_aoi_parity
```

## 顺序契约（两侧必须一致，写死在各自实现里）

1. 实体按场景数组顺序插入 ⇒ **槽位顺序 = AOI 处理顺序**；
2. 每个实体插入时入队一条 `Enter`；
3. 每 tick：`clock += 200` → 追加本 tick 的命令（按场景顺序）→ FIFO 处理队列 → 按插入顺序做 AOI；
4. `Walk`：界内 → 目标格阻挡者（`!Ghost && !Death && !FixedHideMode && !ObMode`）→ `MoveToMovingObject` → 更新坐标；
5. `Leave`：从格子摘除 + 从仓库/ActorMgr 移除；
6. **虚拟时钟**：两侧都不读墙钟（C# 侧把 `HUtil32.GetTickCount()` 桩成由驱动推进的时钟）。

场景 `basic.json` 刻意覆盖：视野边界（dx=5 可见 / dx=6 不可见）、扫描序（X 外/Y 内）、
过滤器（Ghost/Invisible/FixedHideMode/ObMode 各自被排除）、主人规则（master 距离 ≤3 才可见）、
死亡路径（`SearchViewRangeDeath`）、阻挡反直觉项（Ghost/ObMode/FixedHideMode **不阻挡**，
Invisible **阻挡**）、离开世界。

## 与 C# 对齐时确认/踩到的两件事

1. **C# 的 AOI 不排除观察者自己**：`BaseObject.ViewRange.cs:108-116` 的过滤器没有
   `baseObject == this` 判断，自己格里的自己对象会被 `UpdateVisibleGay` 加进 `VisibleActors`
   （金标准第 1 tick 的 `ent 1 30 30 vis 1:2 …` 就是自证）。**照抄不修**（行为等价优先），
   由金标准钉住。
2. **桩的错误会伪装成"两侧不一致"**：第一次对拍差异是 `11:2` 只出现在 Rust 侧——
   根因是我在 C# 桩里**漏了给 `Master` 字段赋值**（Rust 侧主人规则其实是对的）。
   教训：对拍红了先查夹具自身（这次是夹具漏字段），别直接改被测实现。


## M2 下半程（最小切片）：会话进世界 + 掉落系统

范围（刻意收窄，见 `M2骨架报告.md`）：**会话进世界**（进图/切图/掉线/小退/再进）+ **一个系统（掉落）**；
战斗结算（`_Attack` 的 RNG 面）不在本切片的 golden 里——击杀由脚本化 `kill` 触发，
因此随机流只由"掉落预算"消耗，先把逐行一致钉死。

```bash
# 金标准（两个场景）
dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- basic
dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- session
# Rust 对拍
cargo test -p mir2-parity-tests --test world_aoi_parity --test world_session_parity
```

场景 `session.json` 覆盖：进图 → 移动（含被怪物挡住的一步）→ 怪物生成（掉落预算在此消耗 RNG）
→ 脚本化击杀（掉落落地 + 金币）→ 切图 → 小退 → 再进（新 `ActorId`）。

**统一输出格式**（两个场景一致，两侧逐行比对）：

```
tick <t> clock <ms>
ent <id> <map> <x> <y> <hp> <level> <exp> vis <目标id>:<flag> ...
atk <攻击方> <目标> <攻击力> <结算后目标HP>            # 仅 extended 场景（战斗切片）
death <id> <落地件数> <金币> <经验归属者|-1> <获得经验> # 仅 extended 场景
floor <map> <x> <y> <obj_id>                          # 仅 extended 场景
```

对齐过程中修掉的一处真差异：**切图后"再进"必须用会话当前地图**——Rust 测试驱动最初用会话
初始地图（map 0），而 C# 把当前地图记在会话上（切图时更新）⇒ 首个差异就是 `ent 3 0 …` vs `ent 3 1 …`。
两侧驱动统一为"会话当前地图"后逐行一致（`session.txt`）。

## M2 下半程（第二批）：战斗结算 `_Attack` 入 golden（`combat.json`）

上一批把 RNG 面收在"掉落预算"一个系统上；本批把**战斗结算**（`_Attack` 的攻击力 / 命中 /
护甲三条 RNG）也钉进 golden，并把死亡后的**经验归属**纳入对拍。

```bash
# 金标准
dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- combat
dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- combat_exp_nofix
# Rust 对拍（含 2 条红检 + 1 条覆盖判据 + 1 条语义判据）
cargo test -p mir2-parity-tests --test world_combat_parity
```

`combat_exp_nofix.json` 是**判据专用场景**（4 tick，只为钉住第 3 条口径）：
`high_level_kill_mon_fix_exp=false` 且玩家 25 级打 1 级怪 ⇒ `CalcGetExp` 的入参若是"自己等级"
得 **500**（不衰减），若误传怪物等级得 **33**（衰减）——两者可分辨，故该行回退必被 `world_combat_parity`
的第 2 条测试抓到（审查 reviewer-3 的 M4a 变异实测：33 vs 500 判红）。
注：同一变异在 `combat.json`（fix=true、玩家 10 级 < 怪 1+10）下**测不出来**——那是本批补判据的原因。

场景覆盖：攻击力 RNG（`dc=0x1E14` ⇒ 20..30）；命中骰（`RandomByte(目标 SpeedPoint)`，**真的会闪避**）；
护甲 RNG 的两条分支（`ac=0` 一次取数 / `ac=0x0A05` 两次取数）；**目标 `HitPoint == 0` 时整段跳过命中骰**
（`BaseObject.Attack.cs:39`）⇒ 该场景是"RNG 消费次数"的判别器；击杀 → 掉落落地 + 经验；
攻击自己（`IsProperTarget` 假）；攻击已死目标（不再结算，但**仍**登记命中者）。

`attack` 的顺序契约（两侧写死）：`GetBaseAttackPoewr()` → `_Attack(nPower, target)` →
若 `target.Race > ActorRace.Play` 则 `target.SetLastHiter(攻击方)`。

### 本批改掉的 3 处 oracle 口径（含 1 处夹具自造的假行为）

1. **`_Attack` 里没有 `LastHiter` 赋值**（`BaseObject.Attack.cs:30-66` 原文如此）。命中者的登记走
   `SendMsg(targetObject, RM_STRUCK, …)`（同文件 `:61-64`，条件是 `targetObject.Race > ActorRace.Play`），
   由目标下一次 `Run()` 里 `AnimalObject.Operate`（`AnimalObject.cs:118-124`）调 `SetLastHiter`。
   ⇒ 夹具旧版在 `_Attack` 末尾自造了 `targetObject.LastHiter = this;`（**假行为**，已删）；Rust 侧旧版
   在 `attack()` 内部写 `last_hiter`（同错，已删），改为结算后按上述条件登记。
2. **经验归属是 `ExpHitter`，不是 `LastHiter`**（`BaseObject.cs:2429 KillFunc`）。而 `SetLastHiter`
   （`BaseObject.cs:1753`）里 `ExpHitter` **只在为空时**认领 ⇒ 归属者是"**第一个**命中者"，
   粘住到 6 秒过期或该命中者死亡。场景 `combat.json` 造了反例：zombie(3) 先被 hero(1) 命中、
   最后由 mate(4) 补掉 ⇒ 归属者必须是 1（`exp_goes_to_first_hitter_not_last` 断言）。
3. **`CalcGetExp` 的入参是自己等级**：`KillTargetTrigger`（`PlayObject.cs:1643`）写的是
   `CalcGetExp(WAbil.Level, fightExp)`，而 `CalcGetExp`（`BaseObject.cs:712`）内部又拿 `WAbil.Level`
   跟这个入参比 ⇒ `L < L + 10` 恒真 ⇒ **击杀经验不按等级差衰减**（哪怕 `HighLevelKillMonFixExp=false`）。
   Rust 旧版把怪物等级当入参（会在 fix=false 时衰减），已改为照抄调用口径；本条由
   `combat_exp_nofix.json` 覆写 `high_level_kill_mon_fix_exp=false` + 25 级打 1 级怪来**钉住**
   （正确 500 / 误传怪物等级 33，见上）。
4. **`StruckDamage` 的倍率取目标存的 `LastHiter`，不是本次攻击者**（`BaseObject.Base.cs:833-851`）：
   因为命中者登记要等目标下一轮 `Run()` 处理 `RM_STRUCK` ⇒ 本次攻击看到的是**上一次**命中者
   （`combat::LastHitter` + `world::World::attack_once` 里的取值）。默认倍率 10 时数值不可分辨，
   只有非默认 `WarrMon/WizardMon/TaosMon/MonHum` 或同 tick 混职业才可见，故 golden 测不到，
   但口径按 oracle 照抄。
5. **不死系加成不加 `> 0` 守卫**：oracle 是 `nDamage += target.AddAbil.UndeadPower` 无条件相加
   （`BaseObject.Base.cs:802-808`）；`UndeadPower` 为负时也要加。

### 本切片**不做**（明确边界，不是"漏做"）

- **技能**（`MagicManager`）与 `GetAttackPowerHit`/`SkillAttackDamage` 的武器特攻面：不在 `_Attack` 子集内。
- **宝宝击杀**：`KillFunc` 里 `ExpHitter.Master != null` 那一支（经验发给主人）未实现。
- **升级曲线**：场景把 `max_exp` 调大，保证本批不触发升级；`need_exps` 表要与现网 `Exps.conf`
  对齐才有意义，留待升级专用场景（避免在这里抄一张会漂移的表）。
- **`LastHiter`/`ExpHitter` 的墙钟过期**（30s / 6s，`BaseObject.Base.cs:119-132`）：依赖墙钟，本骨架不读墙钟。
  其中"命中者**死亡/隐身**后不再认领经验"这一半**已实现**（`world.rs` 的 `!k.death`，语义等价于
  oracle 在命中者 `Run()` 里的清理）；但"命中者先死、怪物随后被别人击杀"这条时序**尚无 golden 覆盖**
  ——**已接受的覆盖缺口**（accepted risk）：要覆盖它得先让 C# 夹具也按 `BaseObject.Base.cs:119-132`
  逐 tick 清 `ExpHitter`，再造"命中者先死"的场景；当前场景里命中者不会死，故不影响本批判据。
- **不死系加成**（`LifeAttrib == LA_UNDEAD` 时加攻击方 `UndeadPower`）：场景尚未注入该字段。

## M2 ④ tick P99 门禁（`tick_bench`）

```bash
# 绝对门禁（历史行为）：任一档 P99 > 100ms ⇒ RED / exit 1
cargo run -p mir2-world --release --bin tick_bench -- 500 2000 1000 2000 2000 1000
# 相对门禁（推荐给合并门禁）：只判相对量，绝对 P99 降级为报告值
cargo run -p mir2-world --release --bin tick_bench -- \
  --baseline tests/parity/world/p99-baseline-tickbench.txt 500 2000 1000 2000 2000 1000
```

场景/百分位/基线口径写死在 `crates/world/src/bin/tick_bench.rs` 头注释与 `P99复测报告.md` §2/§5
（200×200 地图、实体间隔 2 格、每 tick 1/4 实体走一步、种子 42；改一处基线即不可比）。

- `--baseline` 模式只判**相对量**：每档 3 轮**跨档交错**、取 P50/P99 中位数，`P50 > +10%`
  或 `P99 中位数 > +30%` ⇒ `RED` / `exit 1`；
- **绝对 P99 只在空载窗口有意义**（它是 M5 的验收目标，不是并行环境下的每次合并门禁）
  ⇒ `--baseline` 模式只把绝对 P99 打印为报告值；
- **高负载 ⇒ `SKIP-UNDER-LOAD` 且 `exit 0`**（CPU ≥ 50% 或 `rustc`/`cargo`/`cargo-clippy`/`msbuild` ≥ 4 个）；
  但数量级劣化（P50 > +150%）仍判 `RED`——不把负载导致的 RED 当作者的锅，也不让门禁在负载下失去牙齿；
- 缺档 ⇒ `SKIP`（不静默当通过）；读基线文件失败 ⇒ `RED`。

阳性/阴性对照与逐条口径见 `P99复测报告.md` §5（含为什么不是"同进程 A/B"的结构性理由）。

## D3 成本基线（`drop_bench`）

```bash
cargo run -p mir2-world-bevy --release --bin drop_bench                 # 三条路径 + 输出一致性自检
cargo run -p mir2-world-bevy --release --bin drop_bench -- --redcheck    # 追加"每只怪建一次 world"红检路径
```

口径写死在 `crates/world-bevy/src/bin/drop_bench.rs` 头注释（32 只怪/次、2000 次、预热 100 次、
种子 42、`MonRandomAddValue 40`；改一处基线即不可比）。现行基线 = `d3-drop-bench-rebuild.txt`
（历史 `d3-drop-bench.txt` 的生成器从未入库，仅作对照），对账与"派生结论更正"见 `D3成本复现报告.md`。
注意口径：掉落预算在**怪物生成时**一次性预算，**不是每 tick** 一次。
