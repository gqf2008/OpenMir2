# M3⑨-2 第③片：`CM_SPELL` 诱惑（20）接线 证据

- 卡：`openmir2-m3-pet-slave-summon`（M3⑨-实现 2，**第③片**，接完 20 ⇒ 卡内技能面收口）。
- principal：`openmir2-worker-20`；worktree `m3hp`；分支 `worktree-m3-pet-slave-summon`（本片把该支推进到新 head）。
- base：`refs/heads/master` = `d5be14f40eea1ab4e408b5aa055ba02ae3b367da`（⑨-3 已合入；开工前 `git fetch` 后同点）。
- 代码提交：`c6c9f0928e97fdedbd857548346893cb468cd5cd`（证据提交在其上，交付 `--head` 见 patch entry）。
- **F-1 返工**（reviewer-6 `7724b9e441` request_changes）：由 `11c98bf84ef9413…` 收口
  （`evidence-m3-pet-slave-f1-rework-20261013/`）；本目录的 `gates/*` 读数与 `SHA256SUMS.txt`
  已按返工后树重记（其余证据件不变）。
- 写集：`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**` + `mir2-rs/tests/parity/m3/**`（证据）。
- 边界：`git diff --name-only d5be14f40 HEAD -- src sql MirClient` = **空**（见 `04-boundary.txt`）。
- 未起栈、未占端口、未 merge、未自签。

## 0. 一句话结论

第②片把召唤面（17/30/41）接上后，20（`SKILL_TAMMING` 诱惑）因 `World::tame_adopt` 未合入而登记挂起。
⑨-3 已合入 ⇒ 本片在 `handle_spell_cmd` 补 20 支：`CretInNearXy` 目标修正 → `MagicAttackRage` 距离门 →
`RM_SPELL` 广播 → **`IsProperTarget && !Animal` 门** → `mag_tamming`（取数账本由既有纯函数逐字保留）→
`TargetCret` 清 → 受奴时 `World::tame_adopt` 落尾（Master / MasterRoyaltyTick / SlaveMakeLevel /
MasterTick / WalkSpeed / NextHitTime / BreakHolySeizeMode / SlaveList.Add）→ 非受奴支 `HP=0` →
共享尾 `RM_MAGICFIRE` / `RM_MAGIC_LVEXP`。`Animal` 用构造期 race 等价物
（`crates/world::combat::race_is_animal`）表达。

## 1. oracle 锚点
见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（DoSpell/SKILL_TAMMING 支/MagTamming 全支/
`IsProperTarget`/`Animal` 各 ctor/`NoTame`/共享尾/帧，逐条 `文件:行号`）。

## 2. 改前 / 改后
见 [`02-change-summary.txt`](02-change-summary.txt)：本片独立净改动 3 文件 +452/-1
（`main.rs` 20 支、`slave.rs` `cast_tamming_spell`+3 判据、`combat.rs` `race_is_animal`+1 判据）。

## 3. 判据（全离线；零起栈/零端口）
见 [`03-tests.txt`](03-tests.txt)、[`gates/14-slave-tests.txt`](gates/14-slave-tests.txt)、
[`gates/15-world-animal-tests.txt`](gates/15-world-animal-tests.txt)：
- **`!Animal` 门**：门不过 ⇒ 连 `MagTamming` 都不进、**零取数**、`boTrain=false`；
- **受奴落尾**：`Master`/`SlaveList`/`SlaveMakeLevel`/`MasterRoyaltyTick` 下界/HP 不 `/10`/`TargetCret` 清；
- **不重复受奴**：已受奴样本走早退支 ⇒ 不重复落尾；
- **Animal race 集合**：51/52/53/82/84/85/90 真、其余假（54 属复活支，登记）。
- 回归：`cargo test --workspace --no-fail-fast` = **98 suites · 1024 passed · 0 failed · 21 ignored**。

## 4. 改坏必红（4 条）
见 [`redchecks/`](redchecks/)（索引 + 4 对原始件）：RC1 删 `tame_adopt` 调用；RC2 删 `!Animal` 门；
RC3 把受奴判定退化为 `out.target_master_is_caster`；RC4 删 `TargetCret` 清。
均 mutated **rc=101 / 目标用例 FAILED**、restored **rc=0**、还原后 `git status` 干净。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`，`--offline`）
见 [`gates/`](gates/)：
- `10-fmt.txt` rc=0
- `11-clippy.txt` rc=0（`--workspace --all-targets -- -D warnings`）
- `12-msgcodegen.txt` rc=0（604 条）
- `13-workspace-test.txt` rc=0（**98 suites · 1024 passed · 0 failed · 21 ignored**）

## 6. 未接（登记，别按常识补）
1. **`RM_MAKEHOLYSEIZEMODE` 帧**（`MagicManager.cs:712/812`）：`crates/protocol` 有常量、无编码器
   ⇒ 状态面无副作用、帧未发（登记）。
2. **`OpenCrazyMode` 狂暴态/时长**（`:797/805`）：本骨架无 `CrazyMode` 字段；对应抽数已在
   `mag_tamming` 内消费（RNG 账本不变），状态未建模（登记）。
3. **`CanReAlive` / `MonGen.ActiveCount`**（`:743-756`）：同 ⑨-3 登记（`MonGen` 未建模）。
4. **`NoTame`**：oracle `MonsterObject.NoTame = MonsterInfo.boTame`（`WorldServer.MonGen.cs:911`），
   而 `boTame` 全仓无赋值点、DB `monsters` 无该列 ⇒ 恒 false；Rust 传常量 false（登记）。
5. **race 54 的 `Animal = true`**（`BaseObject.ReAliveEx` `:2601-2603`）：复活路径未建模（⑨-3 登记）
   ⇒ 不把一个 race 的 `Animal` 永久置真（登记在 `race_is_animal` 注释）。
6. **`(MonsterObject)targetObject` 强转**：oracle 对玩家目标会抛 `InvalidCastException`（被 `DoSpell`
   的 try/catch 吞 ⇒ 返回 false、尾不跑）；本骨架无类型区分，用 `IsProperTarget` 保证实际不可达
   （玩家目标走不到 `MagTamming`）——登记，不额外造异常。
7. **`FastTrain`**：oracle 由「快速修炼项链」置位（`PlayObject.cs:2168`），本仓未装载 ⇒ 恒 false（登记）。
8. **真机对拍**：`run_hero_pet_pair.ps1`（⑨-5 夹具）需独占窗口，本片零起栈 ⇒ **not_verified**。
