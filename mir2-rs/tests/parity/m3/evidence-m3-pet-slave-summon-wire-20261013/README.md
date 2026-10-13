# M3⑨-2 第②片：`CM_SPELL` 召唤面接线 证据

- 卡：`openmir2-m3-pet-slave-summon`（M3⑨-实现 2，**第②片**）。
- principal：`openmir2-worker-20`；worktree `m3hp`；分支 `worktree-m3-pet-slave-summon`。
- base：`refs/heads/master` = `6f0bca48ee0f759dc66d17b352188ed2410c3523`（开工前 `git fetch` 后与
  `origin/master` 同点；交付 head 的父链含 origin/master）。
- head：`06c383b2024be6cc0bb609ba8490e11d1396432b`（= 本地提交 `a6d64adb` + merge `origin/master`）。
- 写集：`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**` + `mir2-rs/tests/parity/m3/**`（证据）。
- 边界：`git diff --name-only 6f0bca48e HEAD -- src sql MirClient` = **空**（见 `04-boundary.txt`）。
- 未起栈、未占端口、未 merge、未自签。

## 0. 一句话结论

把 `CM_SPELL(3017)` 的生产路径**接到奴隶召唤/诱惑的效果体**（第①片已把判定层与
`summon_from_spell` 备好）：`handle_spell_cmd` 在 `default` 支完成
`CretInNearXy` 目标修正 → `MagicAttackRage` 距离门 → `SendRefMsg(RM_SPELL)` 广播 →
**护身符门/消耗**（`CheckAmulet`/`UseAmulet`）→ `summon_from_spell`（17 骷髅 / 30 神兽 / 41 精灵）→
`boSpellFire` 的 `RM_MAGICFIRE` → `boTrain` 熟练度尾 `RM_MAGIC_LVEXP`。
同时补齐四个 `SM_*` 帧编码器与 `magics` 模板表生产加载、`[Setup] MagicAttackRage`。

**片内范围界定**：本片接 **17/30/41 召唤面**；**20（诱惑）** 的受奴尾需要
`World::tame_adopt`，而该尾落在 `openmir2-m3-pet-slave-lifecycle`（⑨-3，**待审**）里
⇒ 待 ⑨-3 合入后由小补片接（见 §6）；**46（分身）** 按 `crates/world::slave::CLONE_DEFERRED`
登记（造玩家型克隆体，与宝宝系统不同轴）。护身符**扣光支**的 `SM_DELITEMS` 帧尚无编码器 ⇒
状态照改（清槽）、帧面登记。

## 1. oracle 锚点
见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（`文件:行号` 逐条，含 DoSpell/CheckAmulet/
UseAmulet/四帧/配置）。

## 2. 改前 / 改后
见 [`02-change-summary.txt`](02-change-summary.txt)（8 文件 +1067/-30）。

## 3. 判据（全离线；零起栈/零端口）
见 [`03-tests.txt`](03-tests.txt) 与 [`gates/14-slave-tests.txt`](gates/14-slave-tests.txt)：
- **护身符门**：`cast_without_amulet_is_spell_fail`（门不过 ⇒ false、无 MAGICFIRE、不消耗）；
  `cast_with_amulet_consumes_and_reaches_summon`（过门 ⇒ 450→350、boSpellFire、Rejected ⇒ boTrain=false）。
- **帧字段位**：`smframe::tests::spell_and_magicfire_field_layout`、
  `smframe::tests::magic_lvexp_and_durachange_field_layout`。
- **端到端接线**：`tests::spell_cmd_summon_wires_amulet_and_frames`（观察者收 SM_SPELL+SM_MAGICFIRE；
  自身收 SM_DURACHANGE+`+GD/`；UseItems 耐久 350）。
- **纯函数**：`magic::amulet_use_tests::use_amulet_deducts_when_enough_and_zeroes_when_not`（边界逐点）。
- **回归**：`magic_effect_vectors`（6 passed，royalty_draw 表面积化未改 RNG 账本）。

## 4. 改坏必红（4 条）
见 [`redchecks/`](redchecks/)（索引 + 4 对原始件）：RC1 `use_amulet` 边界改 `>=`；RC2 `sm_spell` Series
取错；RC3 不消耗护身符；RC4 距离门传 0。均 mutated rc=101/FAILED、restored rc=0、还原后 `git status` 干净。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`，`--offline`）
见 [`gates/`](gates/)：
- `10-fmt.txt` rc=0
- `11-clippy.txt` rc=0（`--workspace --all-targets -- -D warnings`）
- `12-msgcodegen.txt` rc=0（604 条）
- `13-workspace-test.txt` rc=0（**97 suites · 994 passed · 0 failed · 21 ignored**）

## 6. 未接（登记，别按常识补）
1. **20 诱惑**：`MagTamming` 的受奴尾（`MagicManager.cs:704-825` 的 `Master=…; MasterRoyaltyTick=…;
   SlaveMakeLevel=…; SlaveList.Add`）落在 ⑨-3 的 `World::tame_adopt`（待审）。本片已把该尾要用的
   `Random(caster.Lv*2)` **表面积化**（`TammingOutcome.royalty_draw`）并登记 `SKILL_TAMMING`；
   ⑨-3 合入后本片补一行派发即可（登记在卡上）。
2. **46 分身**：`MagMakeClone` = `new PlayCloneObject(PlayObject)`；见 `crates/world::slave::CLONE_DEFERRED`。
3. **护身符扣光支的 `SM_DELITEMS` 帧**：`UseAmulet` else 支（`MagicBase.cs:124-126`）无编码器 ⇒
   状态照改（`UseItems[slot]=None`）、帧面未发（登记）。
4. **DoSpell 版本门**（`MagicManager.cs:167` `SoftVersionDateEx==0 && ClientTick==0`）：会话未建模这两个
   客户端字段 ⇒ 未接（登记）。
5. **限速 / `SpellTick -= 450` / MP 消耗**：属 `magicwire` 模块头既有登记（第二片未接），本片未动。
6. **`FastTrain`**：oracle 由「快速修炼项链」置位（`PlayObject.cs:2168`），本仓未装载该面 ⇒ 恒 false（登记）。
7. **真机对拍**：`run_hero_pet_pair.ps1` 需独占窗口（⑨-5 夹具），本片零起栈 ⇒ not_verified。
