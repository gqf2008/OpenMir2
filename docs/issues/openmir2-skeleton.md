# openmir2-skeleton — 17 `SKILL_SKELLETON`（骷髅召唤）对拍卡

**状态**：Rust 侧对拍面（驱动 + 专用比较器）已写，**待编译 + 绿跑回填证据**；未升格（manifest `MG-24` 仍 `implemented_unverified`，duplicate `MG-31`）。
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

## 5. 证据（待绿跑回填）
- 驱动：`tests::skeleton_parity_summon_ok`(id_base 4000) / `tests::skeleton_parity_cap_hit`(5000)。
- 比较器：`real_rust_ledger_skeleton`（两例各自独立结论）。
- 待回填：`cargo check` rc、两驱动 passed、比较器 `[skeleton summon_ok] PASS` / `[skeleton cap_hit] PASS`、三断逐条、桩态（`git status` 空 / `MUTATION`=0）。

## 6. 未覆盖面
- **神兽（30）/ 精灵（41）** 的召唤面：`summon_slave` 已含 `Recalled`/`Blocked` 分支，但本卡只验 17。
- **造怪抽数面**（`Rng` / `640` / `10123-p3`）：离线替身缺 `MonsterInfo` 掉落表（`MonGetRandomItems`）/`CoolEyeCode`，不可复刻 ⇒ 不在对拍范围；生产抽数需真装置另开。
- **`CheckAmulet` 失败 → 639** 支；**`CheckServerMakeSlave` 拦截（Blocked）** 支。
