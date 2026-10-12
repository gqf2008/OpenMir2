# M3④-实现 C 第 ③ 片 —— 怪物帧面（monster ActorDisplay + AOI 出现/死亡帧）（证据）

- 卡：`openmir2-m3-monster-ai-tick`（M3 逐系统对齐 ④ 的承接卡 C）
- 分支 `monster-ai-host`；base = `refs/heads/master` `e2b94234`（交付前 merge origin/master）；head 见卡上 `patch`
- 叠加：本 head 在**分片②**（head `a5d8d579`，patch `71ced740ff8f02232734e63f68671894a3ecf638`，待审）之上；
  **本片增量 = `f7725161..<head>`**（5 个代码文件 + 本目录）
- 写集：`mir2-rs/bins/game-svr/**` + 本证据目录；**零** `src/ sql/ MirClient`、**零** `crates/world/**`

## 本片做了什么

分片② 把生成期取数（`dir`/`cool_eye`/构造计时器）落进 `Entity.ai` 并开了 AI 闸门，但
`drain_aoi_downlink` / `drain_combat_downlink` **对没有展示表的 actor 跳过并 WRN** —— 怪物因此
在观察者屏幕上完全不存在。本片把「怪物的展示层」按 oracle 补齐并接进两处下行：

| 文件 | 内容 |
| --- | --- |
| `bins/game-svr/src/feature.rs` | `make_monster_feature(race_img, weapon, appr) = MakeLong(MakeWord(raceImg, weapon), appr)`（`M2Share.cs:1960`） |
| `bins/game-svr/src/downlink.rs` | `gbk()`（唯一 GBK 编码实现）、`filter_show_name()`（`M2Share.cs:571`）、`monster_display()`（feature/light/status/feature_ex/name_color/show_name）、`MONSTER_NAME_COLOR=255` |
| `bins/game-svr/src/enter.rs` | `enter::gbk` 改为委托 `downlink::gbk`（一份实现） |
| `bins/game-svr/src/mongen.rs` | `WorldMonsterSink` 增 `displays`；`spawn_monster` 成功后登记怪物展示层 |
| `bins/game-svr/src/main.rs` | `step_mon_gen` 透传 `displays`；AOI 出现帧/死亡帧对非玩家 actor 用世界当前 `Entity.ai.dir`；新增 `new_player_entity()`（`WantRefMsg=true`，`PlayObject.cs:889`）用于生产进世界 |

**关键点**：`MonsterWeapon = HUtil32.LoByte(monster.MP)`（`WorldServer.MonGen.cs:902`）；出现帧的 `Dir`
取世界当前 `ai.dir`；`WantRefMsg=true` 是「看到怪」的可见门（`BaseObject.ViewRange.cs:112`）。

## 判据（全构造/进程级，7 条新用例）

1. `feature::tests::monster_feature_byte_layout_is_race_img_weapon_then_appr`
2. `downlink::tests::filter_show_name_truncates_at_first_digit_or_dash`
3. `downlink::tests::monster_display_matches_oracle_fields`
4. `downlink::tests::monster_aoi_turn_pair_bytes`
5. `mongen::tests::world_monster_sink_registers_monster_display`
6. `tests::new_player_entity_sets_want_ref_msg`
7. `tests::aoi_show_emits_monster_turn_pair_with_world_dir`（端到端：玩家 + 近距怪 → 观察者收到
   **怪物 ActorId** 的 `SM_TURN + SM_FEATURECHANGED`，`Series` 取世界当前 `ai.dir`）

读数：`cargo test -p game-svr --offline` lib **109 passed** / main **159 passed**；
`cargo test --workspace --offline` = **86 suites · 769 passed · 0 failed · 18 ignored**（基线 762 ⇒ +7）。
四门禁：`gates/`（fmt rc=0 / clippy rc=0 / msgcodegen 604 rc=0 / workspace rc=0）。

**改坏必红 4 条**（`redchecks/`，全 rc=101）：RC-C3-A 删展示登记 / RC-C3-B `LoByte`→高字节 /
RC-C3-C 出现帧朝向退回展示表初值 / RC-C3-D 不置 `WantRefMsg`。

## 残留（登记，交后续分片）

- **怪物移动/转向帧**：`TickRecord` 只暴露 `processed/aoi_order/attacks/deaths`，无怪物移动事件；
  出现帧之后怪物的走动不逐格下发（属移动同步分片，未做）。
- `MonsterObject.GetChrColor` 的 `CrazyMode/HolySeize` 特例、`Master!=null ⇒ SlaveColor[]`（宝宝系统）未建模。
- `NpcNameColor`（`Race==10`）本片不可达（`race_is_spawnable` 不含 10）。
- `RM_DISAPPEAR` 无编码器（上一片已登记）。
- **真机**：第二客户端/假人进屏 ⇒ 观察者收怪物帧的截图判据等 owner 发窗口（起栈禁接管道），与卡 D 合并跑。

## 复现

见 [`commands.txt`](commands.txt)。证据 hash 复核：

```sh
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/world/monster-ai-frames-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs
```
