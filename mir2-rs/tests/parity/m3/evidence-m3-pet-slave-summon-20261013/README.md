# M3⑨-实现 2 宝宝召唤（`openmir2-m3-pet-slave-summon`）证据

- 卡：`openmir2-m3-pet-slave-summon`（M3⑨-实现 2）。
- principal：`openmir2-worker-20`；worktree `m3hp`；分支 `worktree-m3-pet-slave-summon`。
- base：`refs/heads/master` = `d4d5b89c73e77ed995188c2668dc3eda9a926dc0`（开工前 `git fetch` 后与
  `origin/master`/`walgit/master` 同点）。
- 写集：`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**` + `mir2-rs/tests/parity/m3/**`（证据）。
- 边界：`git diff --name-only master HEAD -- src sql MirClient` = **空**（见 `04-boundary.txt`）。
- 盘点输入：`mir2-rs/tests/parity/m3/M3⑨英雄宠物现状盘点报告-20261013.md` §1.B/§1.F。
- 未起栈、未占端口、未 merge、未自签。

## 0. 一句话结论

把 oracle 的**宝宝召唤判定层**1:1 落到 `crates/world::slave`：
`MakeSlave` 的选型/门/落尾、`CheckServerMakeSlave`、`RecallSlave` 的选靶与 `SpaceMoveGetRandXY` 取数、
`MagMakeSlave`/`MagMakeSinSuSlave`/`MagMakeAngelSlave`（技能 17/30/41）的**完整编排**，
并在 `bins/game-svr` 落 `RegenMonsterByName` 的宿主接缝（[`MonsterTableSpawner`] → 复用 ④-B
`create_monster_oracle` 的**同一条** RNG 流）。含两条 oracle 缺陷照搬（骷髅选型无 `break`；
神兽 `nMakeLevel` 算了不传）。

**本片是 ⑨-2 第①片**：判定层 + 宿主接缝 + 判据已闭合；`CM_SPELL(3017)` 的**效果体派发**（含
`CheckAmulet`/`UseAmulet` 护身符面与 `RM_SPELL`/`boTrain` 帧尾）作为第②片登记未接（见 §6）。

## 1. oracle 锚点
见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（`文件:行号` 逐条）。

## 2. 改前 / 改后
见 [`02-change-summary.txt`](02-change-summary.txt)。

## 3. 判据（构造级，全离线；零起栈/零端口）
见 [`03-tests.txt`](03-tests.txt) 与 [`gates/14-slave-tests.txt`](gates/14-slave-tests.txt)：新增 21 条用例全绿。

- **选型**：`skeleton_selection_keeps_last_match_no_break`（oracle 无 break ⇒ 最后一个命中项）、
  `skeleton_selection_stops_at_zero_hum_level`、`sinsu_selection_breaks_on_first_match`、
  `sinsu_selection_falls_back_to_dragon_name`（`nExpLevel>4` 的 Dragon 名拼接）、
  `angel_uses_magic_level_for_both`。
- **门**：`make_slave_gate_is_strictly_less`、`check_server_make_slave_blocks_only_on_pending`。
- **落尾**：`make_slave_adopt_writes_oracle_tail`（master/IsSlave/MasterRoyaltyTick/SlaveMakeLevel/
  SlaveExpLevel/HP 补半/入 SlaveList）、`make_slave_adopt_keeps_full_hp`。
- **回召取数**：`recall_slave_target_scans_from_tail`、`space_move_returns_start_without_rng_when_walkable`、
  `space_move_steps_by_n18_when_blocked`、`recall_slave_moves_tail_match_to_front`、`recall_slave_no_match_is_noop`。
- **编排**：`summon_slave_skeleton_end_to_end`（前排位置 + 落尾）、`summon_slave_rejected_when_slave_list_full`
  （门拒 ⇒ 不调 spawner）、`summon_slave_sinsu_falls_back_to_recall`（神兽 `MakeSlave==null ⇒ RecallSlave`）、
  `summon_slave_ignores_other_skills`。
- **game-svr**：`pet_slave_config_reads_conf_and_ignores_bom`、`pet_slave_config_missing_file_errors`。

## 4. 改坏必红（4 条，mutation → FAILED → 还原）
见 [`redchecks/`](redchecks/)（索引 + 4 份原始件）：RC1 门 `<=`（3 红）、RC2 骷髅补 `break`（1 红）、
RC3 HP 补半 `/3`（2 红）、RC4 回召改从头找（1 红）。还原后 `slave.rs` sha256 与提交一致。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`，`--offline`）
见 [`gates/`](gates/)：
- `10-fmt.txt` rc=0
- `11-clippy.txt` rc=0（`--workspace --all-targets -- -D warnings`）
- `12-msgcodegen.txt` rc=0（604 条：Messages.cs 594 + Grobal2 白名单 10）
- `13-workspace-test.txt` rc=0（**94 suites · 917 passed · 0 failed · 21 ignored**；基线
  `d4d5b89c` = 94/896/0/21 ⇒ 本片 +21 用例）

## 6. 未接（登记，别按常识补）

1. **`CM_SPELL(3017)` 效果体派发**：oracle 的 17/30/41 在 `DoSpell` 的 `switch` 里
   （`MagicManager.cs:313-322/450-470`），前置 `CheckAmulet`/`UseAmulet`（17/41 需 1 个 type-1、
   30 需 5 个）与 `RM_SPELL`/`boSpellFire`/`boTrain` 尾。本片把召唤编排与宿主 spawner 都备好
   （`bins/game-svr::slave::summon_from_spell`），但**未**改 `main.rs::handle_spell_cmd`——原因：
   该文件与在审的 ⑤-4（`shop-wire-host`）/⑦-2（`party-group-host`）写集重叠，且护身符面
   （`UseItems[ArmRingl/Bujuk]` + `ItemSystem.GetStdItem`）本身是独立的一块。⇒ 归 ⑨-2 第②片。
2. **诱惑(20) 成功后的受奴落尾**：`MagTamming`（`magic.rs:548-819`）已实现取数/判定，但
   `target.Master = playObject` 一族（`MagicManager.cs:751-777` 的 royalty/封顶/`MasterTick`）未接。
   归 ⑨-3（`MasterRoyaltyTick`/`CheckRoyalty` 同卡）。
3. **`RecalcAbilitys` 派生类速度改写 + `RefNameColor` 色彩**：`WhiteSkeleton.cs:21-27`、
   `ElfMonster.cs:31-42`、`ElfWarriorMonster.cs:35`、`BaseObject.Base.cs:869-881` 的
   `GetChrColor`/`SlaveColor`。归 ⑨-3。
4. **`RecallSlave` 的出帧**：`SpaceMove` 的 `RM_CLEAROBJECTS`/`RM_CHANGEMAP`/`RM_SPACEMOVE_SHOW2`
   （`BaseObject.cs:1134-1144`）。本片只做格子移动 + `SpaceMoveGetRandXY` 取数；帧面归宿主接线片。
5. **46 分身**：`MagMakeClone` = `new PlayCloneObject(PlayObject)`（造玩家型克隆体、不入 `SlaveList`），
   与宝宝系统不同轴 ⇒ 登记未实现（`crates/world::slave::CLONE_DEFERRED`）。
6. **`CheckServerMakeSlave` 的 `RM_10401` 产者**：Rust 无跨服换服面 ⇒ 宿主恒传 `false`（门恒放行）；
   本片保留完整判定以便跨服片接入。
7. **i16 口径**：`GetFrontPosition` 结果按 oracle `short` 截断（`fx as i16`）；`SpaceMoveGetRandXY`
   的 `nX += n18` 在 oracle 是 `short` 运算（本实现用 `i32`，超大图才会分叉，登记）。
8. **真机对拍**：需窗口（`run_hero_pet_pair.ps1` 侧），本片不起栈。
9. **`caster_dir` 显式传入**：玩家在宿主侧的权威 `BaseObject.Dir` 由 `downlink::DisplayTable`
   维护（客户端指令），`Entity.ai.dir` 只对怪物由 AI 维护 ⇒ `summon_slave`/`recall_slave`/
   `summon_from_spell` 都取显式 `caster_dir` 参数，不代宿主猜 `Entity.ai.dir`。

## 7. 复现
见 [`commands.txt`](commands.txt)。

## 8. 残留物清单
- `E:/tmp/openmir2-m3hp-target`（隔离 CARGO_TARGET_DIR，保留复用）。
- `/tmp/slave.bak`、`/tmp/slave.good.rs`（红检备份，与还原后文件逐字节相同，可随时清）。
- 本 worktree 干净；共享主检出未写/未编译；未起栈；未占端口；未 merge、未自签。

## 9. 重交记录（merge `origin/master`，2026-10-13）

合并检查点（`comment 6364fabb`）判本片与 master `7febf884` 在
`mir2-rs/bins/game-svr/src/lib.rs` 真冲突（master 的 `pub mod shop;` 与本片 `pub mod slave;`
同插入位）⇒ 作者按并集解（保留 `party`/`shop`/`slave`/`trade` 全量）后重交。

- 新 base = `refs/heads/master 9edfe4b722155a9eb172d5fab1c1641b3f567885`；head = 合并提交
  `3863c475b`（= `e99f49a4` + merge `origin/master` `9edfe4b72`）。净 diff 仍只有本片 5 个代码路径。
- 门禁重跑（隔离 `CARGO_TARGET_DIR`，merge 树）：fmt rc=0 / clippy rc=0 /
  `cargo test --offline --workspace --no-fail-fast` = **95 suites / 966 passed / 0 failed / 21 ignored** rc=0 /
  msgcodegen 604 rc=0。
- 本证据 `gates/10..14`、`04-boundary.txt` 已按 merge 树刷新；`SHA256SUMS.txt` 用
  `verify_sha256sums.py write` 重记（仓库字节口径）。源码（`slave.rs` 等）与红检读数不变。
- head 变化 ⇒ 需新的非作者 `review` 覆盖 `3863c475b`。
