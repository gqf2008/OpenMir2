# openmir2-skeleton — 17 `SKILL_SKELLETON`（骷髅召唤）对拍卡

**状态**：对拍面（驱动 + 专用比较器）+ 变异牙 + 全量门全绿；**未升格**（manifest `MG-24` 仍 `implemented_unverified`，duplicate `MG-31`），待第二方复核。**最终 head `8d8f5f55f`（base master `977d260a1`）。**
**golden**：`mir2-rs/tests/parity/golden/magic/skeleton.json`（两例 `summon_ok` / `cap_hit`），由 17 装置离线替身生成；本卡 `20/24` 版本（见 §2）。

## 1. oracle 事实（C# 参照实现）
- 分派：`MagicManager` 的 `case SKILL_SKELLETON`（外层护身符门 `CheckAmulet(1,1)`+`UseAmulet(1,1)` → `MagMakeSlave` → 尾）。
- 选型：`MagicManager.MagMakeSlave` —— `sMonName=Config.Skeleton`、`nExpLevel=userMagic.Level`、`nCount=Config.SkeletonCount`，按 `SkeletonArray[i].nHumLevel` 覆盖（**无 `break`**，BUG-REPLICA ①）。
- 编排：`mir2_world_bevy::summon_slave`（`crates/world-bevy/src/slave.rs`）—— `CheckServerMakeSlave` 门 → 选型 → `SlaveList.Count < nMaxMob` 门 → `GetFrontPosition` → `RegenMonsterByName`（宿主 `SlaveSpawner::spawn`）→ `MakeSlave` 落尾。
- 落尾：`BaseObject.MakeSlave` → `World.make_slave_adopt`：`Master/IsSlave/MasterRoyaltyTick/SlaveMakeLevel/SlaveExpLevel` → `apply_slave_level_abilitys`（白骷髅/精灵按 `SlaveExpLevel` 抬 `MaxHP`）→ 派生 recalc → **半补** `if HP < MaxHP { HP += (MaxHP-HP)/2 }` → `SlaveList.Add`。

## 2. golden 夹具与 `20/24` 由来（`summon_ok`）
- 离线替身 `OfflineWorldEngine.RegenMonsterByName` 造怪 `race=100`（WhiteSkeleton 真身）、`Abil.MaxHP=17`、`WAbil.HP=17`（`stub_wabil_hp=17`、`stub_abil_max_hp=17`、`stub_monster_race=100`）。
- `apply_slave_level_abilitys`：`chp = 17 + round(17*0.4)*1 = 24`（`SlaveExpLevel=1`）⇒ `MaxHP=24`；半补 `HP = 17 + (24-17)/2 = 20` ⇒ **hp/max_hp = 20/24、level=1**。
- Rust `skeleton_rig` 照此配怪物表（`race=100 / hp=17`），`skeleton_cast_smoke` 实测 `slave_hp=20/24 level=1`，与 golden 逐项一致（**未改夹具**）。
- **ActorId 段**：`SM_638`（`RM_MAGICFIRE`）尾 = EDCode(int32 目标 ActorId)；golden `summon_ok` 尾 `Nghcd?`→`1002`、`cap_hit` 尾 `Zghjd?`→`2002`（对应 `RM_MAGICFIRE.p3`=1002/2002）。驱动 `id_base` 取 golden 的 `1000`/`2000`（`new_layout` 令 target=`id_base+2`）。

## 3. 对拍面（本卡 SCOPE）
`real_rust_ledger_skeleton`（`mir2-rs/tests/parity/tests/magic_fireball_parity.rs`）比较：
- `Skeleton` block 标量：`regen_calls` / `slave_count_before/after` / `pre_slave_count` / `stub_wabil_hp` / `stub_abil_max_hp` / `stub_monster_race`。
- `new_slaves` 逐项：`name / level / hp / max_hp / master_is_caster`（`summon_ok` 比实值 20/24）。
- 造怪前帧 `17/638/642`（按 socket 取多重集，§47 不跨 socket 断言序）。
- `cap_hit` 另三断**各自独立 assert**：断①实体数不增 / 断②`slave_count` 不变 + `new_slaves` 空 / 断③`regen_calls=0`（门 `Rejected`）。
- **不比** `Rng`（离线替身不抽造怪数，与 Rust `create_monster_oracle` 真抽不同源）、`640`/`10123-p3`（训练尾，随造怪抽数不同）。
- SCOPE 本质：**验「给定怪物之后的 adopt 行为」，不是造怪。**

## 4. 接缝坑（可复用：症状 → 根因 → 补法）
- **症状**：召唤施法跑完，`spawn_calls`（`regen_calls`）已 `+1`，但 `slave_count_after == slave_count_before`、无新实体（`frames` 也少），像「spawn 没生效」。
- **根因**：`bins/game-svr/src/slave.rs` 的 `MonsterTableSpawner::spawn` 首句 `let map_name = self.registry.name_of(map_id)?;` —— 测试 `SpellFrameRig` 用**空** `MapRegistry`（`from_entries(.., Vec::new())`）⇒ `name_of(0)=None` ⇒ `spawn` 在造怪前就 `?` 返回 `None`（但 `spawn_calls` 已 `+=1`，故只看计数会误判「spawn 执行了」）。
- **补法**：给 rig 写一张**带表头**的临时 `Map/0.map`（`HEADER_BYTES` 前 2 字节 = 宽 LE、次 2 字节 = 高 LE，再 `w*h*CELL_BYTES`）→ `MapRegistry::from_entries(MapLoader::new(base,"Map","Envir"), vec![MapInfoEntry{ name:"map", file:"0", .. }])`。**不带表头会 `parse_map_bytes` Err ⇒ 该条被 skip（仍 `name_of=None`）**——照 `bins/game-svr` 的 `cm_wantminimap` 测试写法。
- **另一坑（ActorId 尾）**：召唤帧里 `SM_638` 尾编码 int32 目标 ActorId。驱动 `id_base` 若不是 golden 的 `1000`/`2000`，尾就不同（实测 `4002` vs 期望 `1002`）。对齐 `id_base` 即可，勿改 golden。

## 5. 证据（绿跑，2026-11-20）
- `cargo check -p game-svr --lib` = **rc 0**；`cargo check -p mir2-parity-tests --tests` = **rc 0**。
- 驱动（各 1 passed，单独进程 `--exact --test-threads=1`）：`tests::skeleton_parity_summon_ok`（`entities 4->5 slave 0->1 regen=1 frames=7 new=1`）；`tests::skeleton_parity_cap_hit`（`entities 5->5 slave 1->1 regen=0 frames=6 new=0`）。
- 比较器 `real_rust_ledger_skeleton` = **CMP_RC=0**：`[skeleton summon_ok] PASS` / `[skeleton cap_hit] PASS`（两例各自独立结论）。
- 覆盖：`Skeleton` block 全字段、`new_slaves` 实值 20/24（level 1 / master_is_caster true）、帧 17/638/642 全等；`cap_hit` 三断（①实体数不增 5->5 ②`slave_count` 不变 1->1 且 `new_slaves` 空 ③`regen_calls=0`）各自独立成立。
- ActorId 对齐：`id_base` 由 4000/5000 改 1000/2000（对齐 golden 的 `SM_638` 尾目标 ActorId 1002/2002）；此时全绿。
- 桩态：`git status --porcelain` 空、`grep -rn MUTATION`=0。
- **门禁（最终 head `8d8f5f55f` / base `977d260a1`；下列除 workspace 全量外均在 rebase 后 head 上跑）：**
  - `cargo check -p game-svr --lib` = **rc 0**
  - `cargo test -p game-svr --lib` = **`2156 passed; 0 failed; 76 ignored`**
  - `cargo test --workspace` = **`4386 passed; 0 failed; 163 ignored`**（**基于 rebase 前 head `5fc120fb9`**）
  - `cargo clippy --workspace --all-targets -- -D warnings` = **rc 0**（`Finished dev profile in 33.39s`）
  - `cargo fmt --all -- --check` = rc 0；`msgcodegen --check` = 604 条；`docs_anchor --prose` = 绿
- **差异说明甲（`--lib` 计数）**：master `977d260a1` 的 `--lib` = `2156·0·73`；本卡 = `2156·0·76` = **master 73 + 3 ignored**（`skeleton_cast_smoke` + `skeleton_parity_summon_ok` + `skeleton_parity_cap_hit` 三个 `#[ignore]`）。**passed 相等(2156)，无新增 passed。**
- **差异说明乙（workspace 全量口径）**：`4386·0·163` 是 **rebase 前 head `5fc120fb9`（base `f8c0b4b7d`）** 的读数；rebase 后**未重跑全量**，只重跑 `--lib`（见甲）。两者差异同为那 3 个 `#[ignore]`，**无新增 passed**。
- §31 变异牙见 §5b。

## 5b. 变异牙（§31）——红在哪一层（断言原文）
牙执行于 **code head `dac86eb0b`**（base `f8c0b4b7d`）；每颗 `BUILD_RC=0 / CMP_RC=101 / RESTORE=EQUAL / MUT=0`。三断已前移到 block 标量之前 ⇒ ②③ 变异先红在语义层，不回落到 block 标量。

| 牙 | 变异点 | 红在哪一层 | 断言原文 | left / right |
| --- | --- | --- | --- | --- |
| ① 删半补（实现） | `crates/world-bevy/src/slave.rs` `make_slave_adopt`：删 `if mon.vitals.hp < mon.vitals.max_hp { … }` | block 标量 `new_slaves[].hp`（三断放行后才落此层 ⇒ 重排正确之证） | `[summon_ok] new_slaves[0].hp 不等` | `17` / `20` |
| ② gate 恒真（实现） | `slave.rs` `summon_slave`：`let gate_ok = make_slave_gate(...)` → `let gate_ok = true;` | 三断「实体数不增」 | `[cap_hit 断①/实体数不增] 施法前后实体数变化：before=Some(5) after=Some(6)` | `6` / `5` |
| ③ `slave_count_before` 写死 0（**断言自检牙，非实现**） | `bins/game-svr/src/lib.rs` 驱动：`let slave_before = rig.world.slave_count(caster)` → `0usize` | 三断「slave_count 不变」 | `[cap_hit 断②/slave_list 不变] slave_count 变化：before=0 after=1` | `1` / `0` |

- 牙②驱动实测 `[skeleton cap_hit] seed=1 entities 5->6 slave 1->2 regen=1 frames=7 new=1`。
- ②③ 均红在三断、未回落到 block 标量 ⇒ **三断判据本身成立**（非实现问题）。
- ③ 仅自检「`slave_count` 比较能咬」，**不与 ①② 合并计为「实现被验证」**。
- 还原后 `git status --porcelain` 空、`grep -rn MUTATION`=0。

## 6. 未覆盖面
- **神兽（30）/ 精灵（41）** 的召唤面：`summon_slave` 已含 `Recalled`/`Blocked` 分支，但本卡只验 17。
- **造怪抽数面**（`Rng` / `640` / `10123-p3`）：离线替身缺 `MonsterInfo` 掉落表（`MonGetRandomItems`）/`CoolEyeCode`，不可复刻 ⇒ 不在对拍范围；生产抽数需真装置另开。
- **`CheckAmulet` 失败 → 639** 支；**`CheckServerMakeSlave` 拦截（Blocked）** 支。
