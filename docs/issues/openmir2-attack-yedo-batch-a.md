# openmir2-attack-yedo-batch-a

> 状态：**进行中（2026-11-20 立卡）**。来源 = IT-17b 收口后由 `opc-19` 派的「攻击批」，
> 按调研拆成 **A / B / C** 三批（证据面不同），**本卡只做 A**：
> 攻杀剑法的**攻击时第二抽 + `PowerHit` 置位 + `+PWR`**。

## 0. 为什么拆三批

| 批 | 内容 | 判据面 | 依赖 |
| --- | --- | --- | --- |
| **A（本卡）** | `AttackSkillCount -= 1` / arm / 归零重置并**再抽一次** / `SendSocket("+PWR")` | `+PWR` 击次序列 + 第二抽的抽数与抽值 | 无 |
| B | `PowerHit`/`FireHitSkill`/`TwinHitSkill` 的**消费**（模式 3/7/9 的伤害加成与帧）+ `AttackSuccess` 的 `Random(3)+1` 与 `TrainSkill` | 单帧 ident + 伤害数值 | A |
| C | `SkillAttackDamage` 次级攻击（模式 4/5/8/12、MP 门、邻居扫描） | 多目标受击帧 + 各自 HP | 先裁决 `IsProperTarget`/安全区未建模 |

三批的窗口证据互相污染，混一张卡会让判据不可归因。

## 1. oracle 事实（只读核对，2026-11-20）

### 1.1 计数块（`src/M2Server/Player/PlayObject.Attack.cs:942-956`）

```text
if (MagicArr[SKILL_YEDO] != null && UseItems[ItemLocation.Weapon].Dura > 0) {
    AttackSkillCount -= 1;
    if (AttackSkillPointCount == AttackSkillCount) { PowerHit = true; SendSocket("+PWR"); }
    if (AttackSkillCount <= 0) {
        AttackSkillCount = (byte)(7 - MagicArr[SKILL_YEDO].Level);
        AttackSkillPointCount = M2Share.RandomNumber.RandomByte(AttackSkillCount);
    }
}
```

- **位置**：`ClientHitXY` 成功支**末尾**——在 `switch (wIdent)` 的 `AttackDir(...)`（`:907-939`）
  **之后**、体力/魔力扣减（`:957-960`）**之前**，整体在 `:963` 的 `try/catch` 内。
- **与目标无关**：空挥（`GetPoseCreate() == null`）同样经过本块 ⇒ 两条攻击路径都要跑。
- **读点在磨损之后**：`AttackDir` 内部 `AttackSuccess`（`:215-229`）已跑 `DoDamageWeapon`
  （`:1436`，爆掉即 `Dura = 0`、`Index = 0`）⇒ 磨到 0 的**那一击**起，`Dura > 0` 不再成立。
- **一个字节的坑**：`AttackSkillCount` 是 `byte`（`PlayObject.cs:268-269`）；`-= 1` 是回绕减。
- **初值**（唯一写点）：`RecalcHitSpeed` 的攻杀支 `AttackSkillCount = 7 - L`、
  `AttackSkillPointCount = RandomByte(7-L)`（`PlayObject.cs:3703-3706`，每次 Recalc 抽 1 次——IT-17b 3b 已接）。

### 1.1b 两条**行为事实**（不是用例技巧，是 oracle 与 Rust 共同的行为）

1. **arm 判据在递减之后**：`AttackSkillCount -= 1` 先发生，`AttackSkillPointCount == AttackSkillCount`
   比的是**递减后**的值。⇒ `Count = N, PointCount = N-1` 时，**第一击**（`N → N-1`）就 arm；
   把 arm 判在「第 2 击」是错的（本批用例初版就栽在这里，已改正）。
2. **武器耐久是 `ushort` 回绕减，`Dura < damage` 不会破零**：`nDura -= nWeaponDamage` 是
   unchecked `ushort` 运算（`DoDamageWeapon`，`PlayObject.Attack.cs:1447`），`if (nDura <= 0)`
   对 `ushort` 等价 `== 0` ⇒ **只有恰好扣到 0 才破零**；扣穿得到的是大正数（例如 `Dura=1`
   扣 2..6 得 65533..65535），且尾部桶位检查照发 642。本仓 `do_damage_weapon` 逐字照抄并在
   其文档里登记。⇒ 想构造「本击磨到 0」的用例，必须**先量出本击磨损量 `d`，再用 `Dura = d`
   跑**（自校准），不能假设「小耐久自然会被打穿」。

### 1.2 消费侧（**本卡不实现，只登记**）

- `GetHitMode` 模式 3 + `PowerHit ⇒ RM_SPELL2`（`PlayObject.Attack.cs:94-97`）；
- `GetAttackPowerHit` 模式 3 `PowerHit=false; nPower += HitPlus; canHit=true`（`:586-590`）；
- `canHit` ⇒ `AttackSuccess:253-261` 在 `MagicArr[SKILL_YEDO] != null` 且 `Level < 3` 时多抽
  `Random(3)+1`（`CheckSkillProficiency` → `TrainSkill`）。

### 1.3 oracle 崩溃（登记，不照抄）

有攻杀但没装备武器时 `UseItems[Weapon]` 是 `null`（`CharacterObject.cs:89` 只 `new UserItem[13]`）
⇒ `:943` 解引用抛 `NullReferenceException`，被 `:963` 的 `catch` 吞掉：**攻击照常完成，只丢计数块**。
本仓：**跳过 + WRN**，并把跳过次数记进会话（`MagicState.yedo_no_weapon_warns`，可判据）。

## 2. 实现（写集）

| 文件 | 改动 |
| --- | --- |
| `mir2-rs/crates/world-bevy/src/world.rs` | `YedoRuntime` / `YedoEvent`；`SimState.attack_skill`（注入表）+ `pending_yedo`；`World::set_attack_skill` / `drain_yedo`；唯一实现 `yedo_attack_tail_impl(table, rng, actor, weapon_dura)`，被 `attack_once_impl`（武器回写**之后**取 `use_items[Weapon].dura`）与 `attack_swing`（取实体当前值，空挥不磨损）共用 |
| `mir2-rs/bins/game-svr/src/lib.rs` | `handle_hit_cmd` 注入会话态；空挥支**当场排水**；`sim_loop` 在 `tick_head` 之后排水；`drain_yedo_events` 回写 `passives`、置 `power_hit`、arm 时 `SimToGate::SendRaw(b"+PWR")`；跳过支 WRN + 计数 |
| `mir2-rs/bins/game-svr/src/readbook.rs` | `MagicState.power_hit`（A 批**只置不消费**）/ `MagicState.yedo_no_weapon_warns`（观测计数） |

**设计要点**

- **落点必须在 tick 内**（`attack_once_impl`）：oracle 是「每击 A 首抽 → A 第二抽 → B 首抽 → B 第二抽」；
  若放宿主侧（`attack_cmd` 入队之后），同 tick 两击会变成「A2 → B2 → A1 → B1」= R 级账本差异。
- **抽数用 `sim.rng`，不是 `rng_attack`**：oracle 全局只有一个 `M2Share.RandomNumber`，
  `RecalcHitSpeed` 的初抽与这次再抽同流 ⇒ 与 `RecalcHitSpeed` 同一条 `World.rng`。
  `rng_attack` 只在隔离开关（测试设施）打开时接管**攻击路径**取数 ⇒ **登记偏差**：
  isolation 模式下本抽仍在主流的语义。
- **一记一个事件**：`armed` 与 `reset` 可同击成立（`PointCount == 0` 且递减到 0）。
- **宿主写回是唯一副本**：会话 `MagicState.passives` 是 `AttackSkillCount/PointCount` 的持久方，
  世界只是每条命令拿到的一份副本；同一 tick 内连击时第二击读到第一击写回的值（= oracle 同一对象）。

## 3. 判据（A 的范围 = 击次序列 + 第二抽账本）

**`AttackSuccess` 的 `Random(3)+1` 不在 A 的可证范围**：它只在 `canHit == true` 时发生，
而 `canHit` 只由 `GetAttackPowerHit` 在模式 3/7/9 且 `PowerHit/FireHitSkill/TwinHitSkill` 为真时置位
——**消费全在 B**。故 A 的语料里该抽**结构性 0 次**（与 IT-17b「0 抽是结构性」同一论证）。

但「结构性的 0」必须被**钉住**，否则它与「忘了写」不可区分：

- 用例 `yedo_redraw_consumes_exactly_one_draw_at_the_attack_position`：两个同种子（`World::new` 固定 42）
  的世界，A 的那一击归零抽 1 次、B 无攻杀 0 次 ⇒ 断言 **A 的挥砍序列 == B 跳过 1 次后的序列**。
  将来谁在 `AttackSuccess` 里插 `Random(3)+1`，序列立刻错位 ⇒ 红。
- **B 开批的硬前置**（写死，防漏）：B 必须一次接上「`Random(3)+1` + `CheckSkillProficiency`/`TrainSkill`
  的**状态效果**」——只抽不练 = 账本对、状态错。且 B 第一条判据 = 「模式 3 + `PowerHit` + YEDO `Level<3`
  的那一击比普通击恰多 1 抽」，届时上面那条用例必须**显式改期望**（0 → 1）而不是被悄悄放宽。

## 4. 用例清单

世界（`crates/world-bevy/src/world.rs`）：

1. `yedo_tail_counts_down_arms_and_redraws_once` —— 纯缝：`5→4→3(arm)→2→1→0(重置 7-L)`；
   只在归零那一击抽 1 次；抽值 = 同种子重放的第一项。
2. `yedo_tail_skips_without_weapon_and_does_not_draw` —— `Dura == 0` ⇒ 跳过、计数不变、**0 抽**；
   `present == false` / 表里无该 actor ⇒ 连事件都没有。
3. `yedo_redraw_consumes_exactly_one_draw_at_the_attack_position` —— 见 §3（流位置 + 「0 被钉住」）。
4. `yedo_attack_with_target_advances_and_arms` —— 有目标支：每击恰一个事件、第 2 击 arm、排水后为空。
5. `yedo_tail_runs_identically_for_swing_and_attack` —— 同起点 5 击，有目标支与空挥支的
   `(count, armed, reset)` **逐项相等**。
6. `yedo_reads_weapon_durability_after_wear` —— `Dura = 1` 本击磨爆 ⇒ 事件是 `skipped_no_weapon`、
   计数保持注入值（读点必须在磨损之后）。

宿主（`bins/game-svr/src/lib.rs`）：

7. `yedo_drain_writes_back_and_sends_pwr` —— 回写 `passives`、`power_hit=true`、arm 那一击恰一次 `+PWR`。
8. `yedo_no_weapon_skips_and_counts_warn` —— 跳过计数 +1、计数未变、无 `+PWR`。

## 5. 牙（红检，逐支变异）

| 变异 | 应红的用例 |
| --- | --- |
| 删掉第二抽（`rng.random_below`） | 1（`calls==0`）、3（序列不错位） |
| 把抽数挪到递减之前 | 1（抽值重放不符） |
| 读点改成磨损**之前**的 dura | 6（本击会递减） |
| 删掉 `attack_swing` 里的尾巴调用 | 5（两支不等） |
| `armed` 判据改 `>=` / 删 arm 支 | 1、4、7 |
| 宿主删回写 | 7（passives 断言） |
| 宿主删 `+PWR` 发送 | 7（`pwr == 1`） |
| 跳过支改回「照常递减」 | 2、6、8 |
| 宿主删跳过计数自增 | 8 |

变异纪律：快照 → 改 → 跑过滤 → `copy2` 还原 + `os.utime`（防陈旧 mtime）→ 断言 sha 相等 → 复跑基线；
只在本人 worktree 内跑。

### 5.1 首轮变异暴露的两处「牙不咬」（已修，留档）

1. **用例③的探针是空输入**：初版用玩家 `attack_swing` 的返回值当「流位置」观测，但 `player()`
   夹具的 `dc = 0x0A_14`（`Lo=20 > Hi=10`）让 `GetAttackPower` 的区间退化（`power<0` 归零）⇒
   每次挥砍**恒返回 20**，比较的是常量 ⇒ 删掉第二抽（变异 M1）用例照常绿。
   **修法**：探针改用**怪物**（`AttackPowerMode::Base` ⇒ `random_range(0,p+1)`，小范围恰 1 次采样）
   + 非退化区间 `0x28_0A`，并加前提断言 `b[0] != b[1]`（值必须随流变化，否则本牙仍是空输入）。
2. **变异 M3 写错**：初版用「破零后 `index == 0` ⇒ 传 0」模拟「读磨损前」，那等价于「读磨损后」。
   正确变异必须在 `attack_dir_grouped_poison_ws` **之前**缓存 `pre_wear_dura` 并传给尾巴。

**教训（写进本卡，下一批照用）**：变异红判据必须是「用例真的跑了并 FAILED」，编译错误记 MISMATCH；
每颗牙另要回答「退化/空输入形态下它还咬不咬」。

## 6. 偏差登记

1. **无武器 NRE**：oracle 崩溃被 catch 吞掉；本仓跳过 + WRN + 会话计数（见 §1.3）。
2. **isolation 面**：本抽用 `World.rng`（与 `RecalcHitSpeed` 同流），`MIR2_RNG_SEED` 的攻击隔离只接管
   攻击路径其余取数。
3. **`MagicArr[SKILL_YEDO].Level`**：重置用**本击注入时**的等级；oracle 用同一击内可能已被
   `TrainSkill` 提升的新等级——B 批落地训练时一并处理。
4. **空挥与有目标支的混合次序**：空挥是同步结算、有目标支入队到下一 tick；同 tick 内「先有目标、后空挥」
   时两侧取数次序本就不一致（既有偏差，A 未扩大，也未修复）。
5. **`PowerHit` 只置不消费**：A 批写 `MagicState.power_hit`，读点在 B。

### 6.1 登记面自交代（为什么这 5 条**不是漏登记**）

`mir2-rs/tests/parity/whitelist.md` 的规则是「允许的差异必须登记；没登记 = 不过」。本节逐条给出
**处置**，让下一位读者不必推测这 5 条的去向（本批**不新增** `T-*` 编号，也不必先取号）。

| # | 偏差 | 处置 | 依据 |
| --- | --- | --- | --- |
| 1 | 有攻杀但无武器时 oracle 在 `PlayObject.Attack.cs:943` 解引用 `null` 抛 NRE，被 `:963` 的 `catch` 吞掉 | **行为等价，不进白名单** | 崩溃被同一函数的 `catch` 吞掉后，攻击照常完成、**计数块同样丢弃**（`AttackSkillCount` 不递减、不 arm、不再抽、不发 `+PWR`）——与本仓「跳过 + WRN + 会话计数」在**所有可观测面上相同**（帧/线上包/取数）。差异只在 oracle 多一条 `LogService.Error` 日志行，且本仓反而有 WRN。日志不属对拍面。用例 `yedo_no_weapon_skips_and_counts_warn` 把「计数未变 + 0 抽 + 无 `+PWR`」钉住。 |
| 2 | 第二抽用 `World.rng` 而非 `rng_attack` | **隔离面偏差，不进白名单（默认关，不构成对拍差异）** | `MIR2_RNG_SEED` 的攻击隔离是**测试设施**：`SimState::isolate_attack` 缺省 `false`（`World::new` 置 false，只有 `attack_isolation_seed_from_env` 设了才开）。缺省路径下两条流是同一条 `World.rng`，与 oracle「全局只有一个 `M2Share.RandomNumber`」一致 ⇒ 对拍面（默认配置）零差异。若将来把隔离纳入**真机对拍**，届时按 `T-*` 流程取号登记（属设施口径变化，不是本批行为变化）。 |
| 3 | 重置用**本击注入时**的 `MagicArr[SKILL_YEDO].Level`（oracle 可能已在本击内被 `TrainSkill` 提升） | **归 B 批** | `CheckSkillProficiency`（`PlayObject.Attack.cs:200-213`）→`TrainSkill`（`PlayObject.cs:4393`）/`CheckMagicLevelUp`（`:5198`）本批未移植；没有升级 ⇒ 等级在同一击内不会变 ⇒ **当前等价**。B 批落地训练时必须同时处理「本击内升级后重置用新等级」，已写进 §3 的 B 开批硬前置。 |
| 4 | 空挥（同步结算）与有目标支（入队到下一 tick）在同 tick 内的取数次序 | **既有偏差，本批未扩大** | `attack_swing` 同步、`attack_cmd` 入队的拆法早于本批（`openmir2-m3-attack-magic-wiring` 第一片既有），oracle 是两次击各自完整结算。本批只**沿用**该次序（尾巴挂在两条既有路径上），没有新增或放大任何跨击次序差。修它属攻击路径的整体重排，需单独开卡。 |
| 5 | `MagicState.power_hit` 本批**只置不消费** | **归 B 批** | 消费点（`GetHitMode` 模式 3 → `RM_SPELL2`、`GetAttackPowerHit` 的 `nPower += HitPlus` 与清零、`canHit` ⇒ `Random(3)+1`）全部在 B 批范围（见 §0 表与 §3）。A 批只置位 ⇒ 在没有 B 之前，该字段不产生任何可观测差异（无读者）。 |

## 7. 边界

- 不改 `src/` C# 参照；不动 `useitem`/`readbook` 的既有帧序；不实现 B/C 的任何消费。
- B 开批前置见 §3；C 的 `IsProperTarget`/安全区裁决建议在 A 合并后另交。

## 8. 证据（2026-11-20，rebase 后最终树 @ `c8e48dbde`）

**门禁**（`CARGO_BUILD_JOBS=2`；命令均在 `mir2-rs/` 下执行）

| 命令 | rc | `test result:` |
| --- | --- | --- |
| `cargo test -p mir2-world-bevy --lib yedo` | 0 | 6 passed; 0 failed; 0 ignored（878 filtered） |
| `cargo test -p game-svr --lib yedo` | 0 | 5 passed; 0 failed; 0 ignored（2205 filtered） |
| `cargo test -p mir2-world-bevy`（全量） | 0 | lib 884 passed; 0 failed; 0 ignored + 各 integration 目标全 ok |
| `cargo test -p game-svr --lib`（全量） | 0 | **2140 passed; 0 failed; 70 ignored**（2210 total） |
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy -p mir2-world-bevy --all-targets -- -D warnings` | 0 | — |
| `cargo clippy -p game-svr --all-targets -- -D warnings` | 0 | — |

**变异（8 颗，全部对交付树重跑）**：脚本 `E:/UserTemp/teeth_attack_a3.py`，日志 `E:/UserTemp/teeth-yedo-a4.log`。

| 变异 | 应红用例 | 结果 |
| --- | --- | --- |
| M1 删第二抽 | ①、③ | 咬住（两条都 FAILED） |
| M2 抽数上界改常数 7 | ① | 咬住 |
| M3 读点是磨损**之前**的耐久 | ⑥ | 咬住 |
| M4 删空挥支尾巴 | ⑤ | 咬住 |
| M5 宿主删回写 | ⑦ | 咬住 |
| M6 宿主删 `+PWR` 发送 | ⑦ | 咬住 |
| M7 跳过门票失效 | ②、⑥ | 咬住 |
| M8 宿主删跳过计数 | ⑧ | 咬住 |

每颗变异后 `restored_sha_ok=True`（`world.rs = 8474a954…`、`lib.rs = 8c73a164…`）；全部跑完后
基线复绿（世界 6/6、宿主 5/5），工作树 `git status` 空、`grep MUTATION` = 0。
红判据 = **用例真的跑了并 FAILED**；编译错误记 MISMATCH 不算咬住。

**时间线（留档）**

1. 首轮变异：M1 没咬住（用例③的观测是退化区间下的常量）、M3 变异写错（等价于读磨损后）——
   修法见 §5.1。
2. 一次后台重跑被系统按内存压力 SIGKILL，`finally` 未执行、`lib.rs` 残留
   `// MUTATION: no writeback`；已按「`grep MUTATION` = 0 + 对快照 sha」恢复并复核。
3. rebase 到 `2e8dc5797` 时发现 **master 自身 E0061**（IT-12 合并时 4 实参调 5 形参的
   `recalc_session_ability`）——与本批无关，由 `opc-19` 在 master 上修复（`c8e48dbde`），
   本分支 rebase 到该提交后重跑全部门禁与 8 颗变异（即上表）。
