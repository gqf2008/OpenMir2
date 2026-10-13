# M3④-实现 C 第 4 片（余下支：`BaseObject.Run` 怪侧公共段）证据

- 卡：`openmir2-m3-monster-ai-tick`（M3④-实现 C：怪物实体与 AI tick）。
- 分支：`monster-ai-host`（base = 交付时 `refs/heads/master`）。
- 写集：`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**` + `mir2-rs/tests/parity/whitelist.md`（T-32 收窄 1 行）。
- 边界：`git diff --name-only <base> HEAD -- src sql MirClient` = **空**（见 `20-boundary.txt`）。

## 0. 结论（先看这条）

前三片（① 世界实体 AI 核心 / ② 宿主接线 / ③ 怪物帧面）已把 `AnimalObject.Run/Wondering/SearchTarget`
与 `MonsterObject.Run/Think` 落地。本片补上 `BaseObject.Run()`（`BaseObject.Base.cs:24-316`）里
**怪物会执行但此前未建模**的三段 + 两个字段补齐：

1. 恢复段（`:39-66`，含 `HealthTick/SpellTick/AutoRecoveryTick`）；
2. 体力点段（`:105-110`）；
3. `TargetCret` 清理段（`:111-118`）；
4. `LastHiter` 30s / `ExpHitter` 6s 过期（`:119-132`，此前只做结构清理）；
5. 怪物 `WAbil.MP/MaxMP` 装载（`WorldServer.MonGen.cs:896-897`）。

调用时机照 oracle：`monster_ai_tick` 对每只怪先 `monster_run`（AI）再 `base_object_run_monster`
（= `MonsterObject.Run` 末尾的 `base.Run()`，`MonsterObject.cs:255/359`）。恢复段不消费 RNG。

## 1. oracle 锚点
见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（逐段行号 + set 的字段/初值 + HealthSpellChanged 的帧面口径）。

## 2. 改前 / 改后
见 [`02-change-summary.txt`](02-change-summary.txt)。

## 3. 判据（构造级，全离线）
见 [`03-tests-run-common.txt`](03-tests-run-common.txt)：本片新增 6 条用例（返工 +2，共 8 条）全绿：
- `monai::tests::base_recovery_heals_hp_at_fill_time_and_resets_tick`
- `monai::tests::base_recovery_caps_at_max_hp`
- `monai::tests::base_recovery_high_max_hp_does_not_overflow_u16`（**返工·I-1**）
- `monai::tests::base_recovery_heals_mp_at_spell_fill_time`
- `monai::tests::health_tick_negative_costs_one_hp_and_is_gated`
- `monai::tests::target_cret_cleanup_covers_age_death_map_and_distance`
- `monai::tests::target_cret_cleanup_also_removes_visible_actor`（**返工·M-1**）
- `world::tests::expire_hitters_uses_wall_clock_last_hiter_30s_exp_hitter_6s`

## 4. 改坏必红（6 条，逐条 mutation → FAILED → 还原）
见 [`redchecks/`](redchecks/)（`README.md` 索引 + 6 份原始件）：
- RC-M1：去掉恢复段的 `HealthTick >= HealthFillTime` 门 ⇒ `base_recovery_heals_hp_...` 必红；
- RC-M2：去掉体力点段的 `HP > 1` 门 ⇒ `health_tick_negative_costs_one_hp_and_is_gated` 必红；
- RC-M3：`expire_hitters` 退回「只判命中者死/隐身」（旧版）⇒ `expire_hitters_uses_wall_clock_...` 必红；
- RC-M4：停调 `target_cret_cleanup` ⇒ `target_cret_cleanup_covers_age_death_map_and_distance` 必红；
- **RC-I1**（返工）：恢复段退回 `u16` 加 ⇒ `base_recovery_high_max_hp_does_not_overflow_u16` 必红（panic）；
- **RC-M1b**（返工）：`ClearTargetCreat` 退回 `DelTargetCreat` ⇒ `target_cret_cleanup_also_removes_visible_actor` 必红。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-target`，`--offline`）
见 [`gates/`](gates/)：
- `10-fmt.txt` rc=0
- `11-clippy.txt` rc=0（`--workspace --all-targets -- -D warnings`）
- `12-msgcodegen.txt` rc=0（604 条）
- `13-workspace-test.txt` rc=0（**93 suites · 859 passed · 0 failed · 21 ignored**）

## 6. 未建模（登记）
`SuperMan`（`:35-38`）、`HP==0⇒Die`（`:70-73`，**非等价登记**：oracle 恢复段先于 `HP==0` 判定，Rust 由
`settle_deaths` 先判死 ⇒ 罕见次序差异，详见 `01` 的 O-1）、`MakeGhost`（`:86-101`）、
`Master`/`SlaveList`（`:133-215`，归 ⑨-3）、`ShowHp`/`VerifyTick`（`:217-256`）。
理由与行号见 `01-oracle-anchors.txt`；其余在「普通怪、无 master、无任务」的 oracle 路径上不改变可观测行为。

## 7. 复现
见 [`commands.txt`](commands.txt)。

## 7b. 返工记录（reviewer-6 `request_changes` @`ae8be20d`，2026-10-13）

> 背景：本 head 上面 reviewer-5 签了 approve（`ae8be20d`），reviewer-6 出 `request_changes`（I-1 Important +
> M-1/O-1/O-2/O-3）。本返工只做**代码修** + **登记措辞订正**；不改 oracle 语义、不动写集边界。

- **I-1（Important）**：恢复段 `hp + n`/`mp + n` 的 `u16` 加法 ⇒ 溢出带（`MonsterPowerRate` > 10 可把
  `max_hp` 推过 65000）内 debug panic / release 回绕为 0。改为按 oracle 的 `int` 语义算再落 `u16`
  （见 `02-change-summary.txt` §6）；新用例 + RC-I1。
- **M-1（Minor）**：`target_cret_cleanup` 由 `del_target_creature` 改为 `clear_target_creature`
  （= oracle `ClearTargetCreat`：除 `DelTargetCreat` 外还从 `VisibleActors` 移除）；新用例 + RC-M1b。
- **O-1（信息）**：`01`/`02`/本 README 的「`HP==0⇒Die` 由 `settle_deaths` 等价」措辞订正为**非等价登记**。
- **O-3（信息·文档）**：`HealthSpellChanged` 口径补上 `if (ShowHp) SendRefMsg` 半边，文案改为
  「`ShowHp=false` 的普通怪无帧面；`ShowHp=true` 属已登记未建模面」。
- **O-2（信息）**：节拍差异属 ④-C 既有建模（`monster_ai_tick` 每 tick vs oracle `ProcessMonsters` 的
  `RunTime` 节流），本返工不扩面；已在 `02-change-summary.txt` 保留口径。
- 附带：为把本分支持续叠到当前 master，`git merge master --no-edit` 解了 3 处共享文件并集冲突
  （`main.rs` 恢复灌入 ↔ 商人 tick 开关；`world.rs` `set_recovery_fill_times` ↔ `enable_merchant_tick`；
  `whitelist.md` T-32（本片收窄）↔ T-33（master 的 doc-drift 订正）），无语义取舍。

## 8. 残留物清单
- `E:/tmp/m2svr-target`（隔离 CARGO_TARGET_DIR，保留复用）。
- `E:/tmp/m2svr-redcheck-monsterai.py`（红检驱动，临时、未入库）、`/tmp/monai.good*.rs`（mutation 备份，
  与还原后文件逐字节相同，可随时清）。
- worktree 干净；共享主检出未写/未编译；未起栈、未占端口；未 merge、未自签。
