# M3④-实现 C 第 ② 片 —— 宿主 AI 接线（生成期取数落实体 + 生产开闸 + 隐藏门 1:1）（证据）

- 卡：`openmir2-m3-monster-ai-tick`（M3 逐系统对齐 ④ 的承接卡 C；依赖 B=`openmir2-m3-mongen-load-spawn`，已合入 `62fffb2e`）
- 分支 `monster-ai-host`；base = `refs/heads/master` `62fffb2e`；head 见卡上 `patch` 条目
- 写集：`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**` + 本证据目录；**零** `src/ sql/ MirClient`

## 本片做了什么

第 ① 片（`eb508a97`）落了 world 侧 AI 核心，但**生产路径没有把它接起来**，且 B 分片③ 把
`req.dir`/`req.cool_eye` 抽出来后就丢掉了（注册偏差：「AOI 面属承接卡 C」）。本片闭合这条接缝：

| 文件 | 内容 |
| --- | --- |
| `crates/world/src/entity.rs` | `AnimalState::seed_from_spawn(dir, cool_eye, ctor, now)`：把生成期取数（`AttackTick = now - Random(3000)`、`WalkTick`、`SearchTime`、`SearchTick`/`ThinkTick`/`WalkWaitTick`/`RunAwayStart = now`、`BoFearFire`）写进 AI 初始态；新增 `Entity.hide_mode` |
| `crates/world/src/world.rs` | `spawn_monster_precomputed(..., dir, cool_eye)` 调 `seed_from_spawn`（**仍零取数**，见判据 2 的 rng 断言） |
| `crates/world/src/monai.rs` | `search_target` 隐藏门 1:1（`HideMode`，非 `FixedHideMode`）+ `n10 = 999` + 删不可达重复判死 |
| `bins/game-svr/src/mongen.rs` | `WorldMonsterSink::spawn_monster` 转发 `req.dir`/`req.cool_eye`；新增 `enable_production_monster_ai` |
| `bins/game-svr/src/main.rs` | `sim_loop` 起世界后开闸（一行 `glog!` 自证） |

**缺省仍关**：合成 golden 场景不经 `enable_production_monster_ai` ⇒ `ai_enabled=false`、逐字节不变
（第 ① 片的设计理由不变）。生产宿主由 `sim_loop` 显式开启。

## 判据

- **生成期取数落库**（`world::tests::spawn_monster_precomputed_consumes_no_rng`）：`dir`/`cool_eye`/
  `bo_fear_fire`/`AttackTick`/`WalkTick`/`SearchTime`/`SearchTick`/`ThinkTick`/`WalkWaitTick` 逐字段断言，
  且 `world.rng` **一步未动**。
- **宿主 sink 端到端**（`game-svr mongen::tests::world_monster_sink_seeds_ai_from_spawn_request`）：
  临时 5×5 `.map` 建 `MapRegistry` → `WorldMonsterSink::spawn_monster` → 断言 `Entity.ai` 与
  `req.dir`/`req.cool_eye`/`ctor` 一致。
- **开闸**（`game-svr mongen::tests::production_host_enables_monster_ai`）：缺省 false → 调
  `enable_production_monster_ai` 后 true。
- **隐藏门 1:1**（`world monai::tests::search_target_hide_gate_reads_hide_mode_not_fixed_hide_mode`）：
  `hide_mode=true,cool_eye=false` ⇒ 不选；`hide_mode=false,fixed_hide_mode=true` ⇒ 照选；
  `cool_eye=true` ⇒ 破隐身。
- **改坏必红 4 条**（`redchecks/`，rc=101）：RC-C2-A 开闸空函数 / RC-C2-B sink 丢 dir / RC-C2-C 移除
  seed_from_spawn / RC-C2-D 隐藏门退回 FixedHideMode。
- 四门禁：`gates/`（fmt rc=0 / clippy `-D warnings` rc=0 / msgcodegen 604 rc=0 /
  **workspace 86 suites · 762 passed · 0 failed · 18 ignored**）。

## 残留（登记，交后续分片）

- **怪物帧面**：观察者收到怪物的 `SM_TURN`/`SM_FEATURECHANGED` 需要 monster `ActorDisplay`
  （`MonsterObject.GetFeature = MakeMonsterFeature(RaceImg, MonsterWeapon, Appr)`，`RaceImg` 来自
  `MonsterInfo.RaceImg`，见 `WorldServer.MonGen.cs:888`；`GetShowName`/`GetChrColor`；`GetFeatureEx()=0`）
  ——`drain_aoi_downlink` 现对无展示表的 actor 跳过并 `WRN`（`main.rs`），属本卡下一分片。
- **线程节拍**：oracle `ProcessMonsters` 每线程 `Thread.Sleep(20)`，本宿主 200ms world tick
  （设计文档 §4.2 硬约束①；B 分片③ 已登记）。真机 100 杀对拍要窗口（卡 DoD⑤）。
- 构造抽里 `RunTick`（`run_tick_delta`）、`MeatQuality`、`ZilKillCount` 无 `AnimalState` 字段
  （AI tick 不消费）——登记，不假造。

## 复现命令

见 [`commands.txt`](commands.txt)。证据 hash 复核：

```sh
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/world/monster-ai-host-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs
```
