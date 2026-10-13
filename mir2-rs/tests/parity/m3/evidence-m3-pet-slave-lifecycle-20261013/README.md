# M3⑨-实现 3 宝宝生命周期（`openmir2-m3-pet-slave-lifecycle`）

本目录是卡 `openmir2-m3-pet-slave-lifecycle`（M3⑨-实现 3）交付的证据包。
依赖卡 `openmir2-m3-pet-slave-summon`（⑨-2，同写集串行；本片在其 `crates/world/src/slave.rs`
之上叠加生命周期层）。

## 1. 交付内容（oracle 锚点见 `01-oracle-anchors.txt`）

- **死亡/幽灵/换主清表**（`BaseObject.Base.cs:208-217`）：`World::slave_list_cleanup` +
  `slave_list_cleanup_all`；接进 `World::tick()`（`settle_deaths` 之后，空表零动作）。
- **`CheckRoyalty` 叛变/到期**（`MonsterObject.cs:363-397`）：`check_royalty`；接进
  `monai::monster_run` 的 oracle 三个「落到 CheckRoyalty」口（跳过 `Think`/`AttackTarget`
  的提前返回口）。
- **主人死 ⇒ `ProcessSlaveMutiny`**（`PlayObject.Base.cs:331-357`）：`process_slave_mutiny`
  （逐宝宝 1 次 `Random(rate)`，仅在 `LastHiter != null` 时）。
- **主人幽灵 ⇒ `ProcessSlaveGhost`**（`PlayObject.Base.cs:360-370`）：`process_slave_ghost`。
- **诱惑受奴落尾**（`MagicManager.cs:751-777`）：`World::tame_adopt` + `tame_royalty_tick`。
- **色彩**（`BaseObject.Base.cs:869-883`）：`get_chr_color`；`Entity.name_color`
  （ctor 初值 255）+ `ACTOR_RACE_NPC`。
- **神之祈祷窗口**：`World.spirit_mutiny_tick` + `World::activate_spirit_mutiny`
  （`ProcessSpiritSuite` 的 `M2Share.SpiritMutinyTick`）。

## 2. 判据与门禁

- 判据：本片新增 15 条（`03-tests.txt`）；`cargo test -p mir2-world --lib slave` = 34 passed。
- 改坏必红 4 条（`redchecks/`）：mutated rc=101 / 1 failed；restored rc=0。
- 四门禁（隔离 `CARGO_TARGET_DIR`，`--offline`，合并树）：
  - `cargo fmt --all -- --check` rc=0（`gates/10-fmt.txt`）
  - `cargo clippy --workspace --all-targets -- -D warnings` rc=0（`gates/11-clippy.txt`）
  - `cargo test --workspace --no-fail-fast` = **95 suites / 981 passed / 0 failed / 21 ignored** rc=0
    （`gates/13-workspace-test.txt`；⑨-2 返工 head 基线 966 passed ⇒ 本片 +15）
  - `cargo run -p msgcodegen -- --check` 604 rc=0（`gates/12-msgcodegen.txt`）
- 边界：`git diff --name-only master HEAD -- src sql MirClient` 为空（`04-boundary.txt`）。

## 3. oracle 缺陷照搬（`BUG-REPLICA`）

见 `02-change-summary.txt` §「照搬的 oracle 缺陷」4 条：
① 色表 15 项 + `<= SlaveMaxLevel(50)` 的 latent 越界（Rust 落兜底）；
② `MasterDieMutinySpeed==0` 除零（Rust 跳过除法，不可达分支）；
③ `GainSlaveExp` 死代码 ⇒ 击杀不涨级；
④ `(ushort)MakeLong(...)` 丢掉 DC 高字节缩放值（专测钉住）。

> 登记口径：沿用 ⑨-2 的**代码模块头 + 证据 README** 登记（`whitelist.md` 的 T-* 编号归属
> 待 owner 裁决；合并检查点已知 T-45/46/47 段被 ⑨-5/⑧-10/⑤ 夹具卡占用，本片不抢同段以免重复解冲突）。

## 4. 未接 / 登记（不冒充已实现）

帧面（`RefNameColor`/`RefShowName`/`DisappearA`/`RM_MASTERDIEMUTINY`/`RM_MASTERDIEGHOST`/
`RM_SPIRITSUITE` 自投递）、`MakeGhost` 的 `CanReAlive⇒Invisible` 变体、`CrazyMode` 色支、
诱惑诱饵到期抽数的表面积化（归 ⑨-2 第②片 CM_SPELL 接线）、真机对拍（需窗口）。详见
`02-change-summary.txt`。

## 5. 复现

见 [`commands.txt`](commands.txt)。证据 hash 口径：`SHA256SUMS.txt`（仓库 LF/git-blob 口径），
`verify_sha256sums.py verify --check-extra` GREEN + `eolcheck` GREEN。

## 6. 残留物清单

- `E:/tmp/openmir2-m3hp-target`（隔离 CARGO_TARGET_DIR，保留复用）。
- `/tmp/m3hp3-slave.orig.rs`、`/tmp/m3hp3-monai.orig.rs`（红检备份，与提交逐字节一致）。
- 本 worktree 干净；共享主检出未写/未编译；未起栈、未占端口；未 merge、未自签。
