# 卡 `openmir2-m3-recovery-tick`（T-28）：oracle 周期恢复 tick 的 Rust 1:1 复刻

**issue**：`b34b108f8d166aa77bf3b99a33cfbf87cdc5dfbe`
**patch**：`patch --base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host`
**base**：`refs/heads/master`（`3cd60e55`，已 `git merge --no-edit master` 进本分支）
**worktree**：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`）
**写集**：`mir2-rs/bins/game-svr/**`（新增 `src/recover.rs`；`main.rs`/`lib.rs`/`useitem.rs` 接线）——**未动** `crates/**`、`tests/parity/m3/**` 夹具、`src/**`、表结构/脚本/数据格式。

## 0. 结论（一句话）

oracle `PlayObject.Run()` 的周期恢复段（`PlayObject.Message.cs:70-141`）在 Rust game-svr 里
按行复刻：每 world tick 对每个在线角色消费 `IncHealth/IncSpell/IncHealing`，消费后调
`IncHealthSpell`（`BaseObject.cs:952-975`）并回 `SM_HEALTHSPELLCHANGED(53)`。
⇒ T-14 两相固定起点对拍的唯一剩余差异列 `mir2_db.characters.IncHealth`（C#=0 / Rust=20）
在 Rust 侧归零：吃药后 HP 已满 ⇒ 首轮恢复 tick 即 `IncHealth=0`（oracle `:127-131`）。

## 1. oracle 逐行口径（实现即照此，见 `recover.rs` 模块头）

| 段 | oracle | 实现 |
| --- | --- | --- |
| 触发布尔 | `!Death && (IncSpell>0 \|\| IncHealth>0 \|\| IncHealing>0)`（`:70`） | `recovery_tick` 首判；`Death` 取自 `mir2_world::Entity::death` |
| 否则重置 tick | `IncHealthSpellTick = GetTickCount()`（`:139-142`） | else 支 |
| 节奏 | `dwInChsTime = 600 - _MIN(400, WAbil.Level*10)`（`:72`） | `600 - level_scaled.min(400)` |
| 到点/折回 | `:73` / `incHealthTick = _MIN(200, now-tick-dw)`；`tick = now + incHealthTick`（`:75-76`） | 同 |
| `Per*` 保底 | `<=0 ⇒ 1`（`:79-90`） | 同 |
| 消费 | `IncHealth<PerHealth ⇒ nHP=IncHealth; IncHealth=0` 否则 `nHP=PerHealth; IncHealth-=PerHealth`；`IncSpell`/`IncHealing` 同构（`:92-122`） | 同 |
| `Per*` 更新 | `PerHealth=PerSpell=Level/10+5`；`PerHealing=5`（`:123-125`） | 同 |
| 生效 | `IncHealthSpell(nHP,nMP)`（`BaseObject.cs:952-975`，`HP/MP` ushort 语义 + 夹 `MaxHP/MaxMP`） | `recover::inc_health_spell`（**唯一实现**，`useitem.rs` 也委托它） |
| 清零 | `HP==MaxHP ⇒ IncHealth=0, IncHealing=0`；`MP==MaxMP ⇒ IncSpell=0`（`:127-135`） | 同 |
| 帧面 | `HealthSpellChanged()`（`BaseObject.cs:700-710`）：`Race==Play ⇒ SendUpdateMsg(RM_HEALTHSPELLCHANGED,…)`；`ShowHp ⇒ SendRefMsg(…)` | `drain_recovery`：自见 + `ShowHp` 的 `SendRefMsgRange` 内广播 |
| 初值 | `CharacterObject()`：`Per*=5`、`tick=GetTickCount()`（`CharacterObject.cs:83-86`） | `Recovery::new(now)`，进世界建会话时 `now_ms()` |

`SM_HEALTHSPELLCHANGED` 载荷照 `PlayObject.Message.cs:1736-1739`：
`MakeMessage(SM_HEALTHSPELLCHANGED, processMsg.ActorId, WAbil.HP, WAbil.MP, WAbil.MaxHP)`
⇒ `param=HP`、`tag=MP`、`series=MaxHP`（`smframe::sm_healthspellchanged`）。

## 2. 用例与红检

| 用例 | 钉住 |
| --- | --- |
| `recover::tests::pooled_drug_is_consumed_over_ticks_when_not_at_full_hp` | 1 级 590ms 节奏、逐轮 `IncHealth/HP/Per*/tick` 序列 |
| `recover::tests::partial_pool_is_zeroed_and_not_reconsumed` | `IncHealth<PerHealth` 支的置零（否则二次消费） |
| `recover::tests::dw_in_chs_time_shrinks_with_level` | `dwInChsTime` 随等级收缩（20 级=400ms） |
| `recover::tests::inc_spell_and_inc_healing_are_consumed_into_mp_and_hp` | `IncSpell→MP`、`IncHealing→HP` |
| `recover::tests::full_hp_clears_the_health_pool` | HP 打满清零（**T-14 归零主路径**） |
| `recover::tests::death_resets_tick_without_consuming` | `Death` 走 else 支 |
| `recover::tests::large_gap_folds_back_at_most_200ms` | 超时折回 `_MIN(200,…)` |
| `tests::recovery_tick_consumes_pool_and_broadcasts`（进程级） | 吃药（池药 Shape 0）⇒ 首轮恢复 tick ⇒ 自见 + `ShowHp` AOI 各收 `SM_HEALTHSPELLCHANGED`；`IncHealth` 清零 |

红检（改坏 ⇒ 必红，原始输出 `red-*.txt`，均已 `cp` 还原、`git status` 干净）：

| # | 改坏点 | 必红用例 |
| --- | --- | --- |
| ① | `recover.rs` 删 `IncHealth<PerHealth` 支的置零 | `partial_pool_is_zeroed_and_not_reconsumed`（left `(3,3)` / right `(3,0)`） |
| ② | `recover.rs` 的 `dwInChsTime` 改常数 590 | `dw_in_chs_time_shrinks_with_level` |
| ③ | `main.rs` 的 `drain_recovery` 回帧 `hp`/`mp` 互换 | `recovery_tick_consumes_pool_and_broadcasts` |

## 3. 门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`，全 `--offline`）

| 门禁 | 结果 | 原文 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates-clippy.txt` |
| `cargo test --workspace` | rc=0，73 suites / **584 passed** / 0 failed / 16 ignored | `gates-test-workspace.txt` |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条常量） | `gates-msgcodegen.txt` |
| `cargo test -p game-svr` ×3 | 各 215 passed / 0 failed | `gamesvr-3run-{1,2,3}.txt` |

## 4. 登记（未建模，不静默）

- **`ShowHp` 的生产可达性**：oracle 里 `ShowHp` 由 `MakeOpenHealth()`（`BaseObject.cs:944`）置位，
  唯一驱动是 `RM_DOOPENHEALTH`（`BaseObject.Base.cs:763`），发点为技能「心灵启示」
  （`MagicManager.cs:438`）⇒ 技能层未移植，生产路径 `show_hp` 恒 `false`。本片照 oracle 保留该支
  （调用方可传 `show_hp`，进程级用例以 `show_hp=true` 钉住），**不**假造技能。
- **`Death`**：取自 `mir2_world::Entity::death`（与移动接线同源）；玩家死亡置位路径属战斗片。
- **未纳入本切片**（各自另有卡）：血石/魔血石（`CharacterObject.cs:160-230` 的 `IncHealth += stoneDura`）、
  buff/`StateTimeArr`、`Run()` 的 `AutoTimer` 段、`SendRefMsg` 的 500ms `VisibleHumanList` 缓存
  （与 `broadcast_feature_changed` 同源地按实时扫描近似）。
- **白名单 T-28**：由 owner 在本片合入后更新/退役（本片只交代码，不动 `whitelist.md`）。

## 5. 证据哈希

`SHA256SUMS.txt` 按仓库字节口径（LF 归一 / git blob）记；
`python -I tools/evidence-hash/verify_sha256sums.py --check-extra <本目录>` = GREEN。
