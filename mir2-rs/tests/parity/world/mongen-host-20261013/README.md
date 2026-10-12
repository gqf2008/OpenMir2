# M3④-实现 B 分片③：MonGen 宿主接线（`bins/game-svr`）

卡：`openmir2-m3-mongen-load-spawn`（M3④-实现 B）。本片 = 第 ③ 片（`bins/game-svr` 接线）。
第 ① 片（`crates/envir` 解析）已合入 master `f4790571`；第 ② 片（`crates/world` 调度/RNG 次序，含
`request_changes` 的 R5-1/R5-2 返工）已合入 master `304126ae`。⇒ 本片 patch 的差量 `master..HEAD`
**只含第 ③ 片**（不再夹带第 ② 片 / R5）。

## 本片做了什么

| 位置 | 内容 |
| --- | --- |
| `bins/game-svr/src/mongen.rs`（新增） | 宿主接线层：配置解析、envir→world 条目转换、`MonsterTable`（`GetMonRace` 口径）、`MonGenHost::{start,step}`、`CreateMonster` 回调的 oracle RNG 次序、`WorldMonsterSink` 落库 |
| `bins/game-svr/src/lib.rs` | 注册 `pub mod mongen;` |
| `bins/game-svr/src/config.rs` | 新增 `[Setup] MonsterPowerRate` 解析（缺省 10，`ServerConf.cs:560` / `GameSvrConf.cs:1476`） |
| `bins/game-svr/src/main.rs` | 启动时载 `monsters`（`monitems::load_monsters`）+ `Envir/MonGen.txt`（`load_mon_gen_entries`）+ 刷怪参数；`sim_loop` 建 `MonGenHost`，每 tick `step_mon_gen` |
| `crates/world/src/world.rs` | 新增 `World::budget_monster_drops`（用调用方给定的流做掉落预算）与 `World::spawn_monster_precomputed`（预置掉落/朝向/构造抽，**不再取随机数**）+ 2 条用例 |
| `crates/world/src/entity.rs` | 新增 `MonsterCtorDraws` + `Entity.ctor_draws`（构造链抽值落实体，供承接卡 C 的 AI 消费） |
| `crates/world/src/lib.rs` | 导出 `MonsterCtorDraws` |

`CreateMonster` 回调的取数次序（`WorldServer.MonGen.cs:498-810`）：
`FindMap==null ⇒ None`（不抽）→ race 不在 switch ⇒ `None`（不抽）→ **构造链取数**
（`BaseObject.cs:439/441` → `AnimalObject.cs:76/77` → `MonsterObject.cs:65`/`ATMonster.cs:10`/… 各层 ctor
+ switch 内联 51/52/53/95/96/97）→ `Dir = RandomByte(8)` → `MonGetRandomItems`（掉落预算）
→ `Random(100) < CoolEyeCode` → 地形移位。

> **F-1 返工（reviewer-6 `07444eef`，owner 2026-10-13 06:05 裁决走 (b)）**：原片只抽 switch 内联的 6 个
> race，**漏了 `new Xxx()` 构造函数链里的取数**（构造在 switch 内同步执行、早于 `cert.Dir`(`:740`)，
> 属生成路径 RNG 流）。本片按 oracle 完整链补齐（逐 race 抽数见 `01-oracle-anchors.txt` §6），并加一条
> **全链次序 + 构造抽值**的判据（`monster_constructor_draw_count_matches_oracle_per_race` /
> `monster_constructor_values_match_oracle_order`），证据 40/41。
> 口径订正：reviewer 示例的 `80=1 / 81=2` 只数了 `MonsterObject`/`ATMonster` 那一次 ctor 抽，
> 漏计公共链 `BaseObject`(2)/`AnimalObject`(2)；按 owner「逐 race 自己读源码确认」按完整链实现。

## 关键口径

- **单条随机流贯穿**：`step_mon_gen` 把 `world.rng` 临时移出交给 `MonGenHost::step`，
  `WorldMonsterSink` 只落地、不再取数；回调在两次位置抽之间同步完成 `CreateMonster` 的取数
  ⇒ **位置抽之间的插入位置与 oracle 的 `RegenMonsters` 调用序一致**（`regen_monsters` 的回调契约）。
  注意：这是「回调插入位置」的对齐，不等于整体 RNG 流已对齐——**整体对齐还需构造链取数**
  （见上面 F-1 返工；原措辞「逐调用相同」已如此限定，不再声称 create 内部次序已对齐）。
- **构造链取值落实体**：`draw_monster_construct` 抽出的值作为
  `Entity.ctor_draws`（`MonsterCtorDraws`）落库（供承接卡 C 的 AI 消费，C 只消费不抽）；
  `dir`/`cool_eye` 只随 `MonsterSpawnRequest` 交给 sink（AOI 面仍属承接卡 C）。
- **RNG 位置判据**：每个 race 分支的用例都用「第二个同种子流**含构造链**手工复算 oracle 次序」对照，
  比较函数返回后的**下一抽**——改错抽数个数/次序即红（见 red-check RC-1/RC-2/RC-4/RC-5）；
  另加逐 race 抽数表与构造抽值逐字段判据（`monster_constructor_*`）。
- **登记差异**：oracle 每线程 `Thread.Sleep(20)`，本宿主每 200ms world tick 驱动一次
  （设计文档 §4.2 硬约束①单线程确定性）；`dir`/`cool_eye` 已取数但 world `Entity` 未建模
  （AOI 面属承接卡 `openmir2-m3-monster-ai-tick`）。

## 判据与门禁（全绿）

- `cargo test -p game-svr --lib mongen`：**14 passed**（RNG 次序 ×7、表/转换 ×2、配置解析 ×1、宿主 start/step ×3；
  含 F-1 新增的 `monster_constructor_draw_count_matches_oracle_per_race` 与
  `monster_constructor_values_match_oracle_order`）。
- `cargo test -p mir2-world --lib`：41 passed（含本片新增 2 条；第 ② 片 R5 的 3 条也在此计数内）。
- **独立 Python 复算**（`40-*.py.txt` / `41-*.txt`）：从 `src/` 解析类继承 + ctor 的 `Random` 调用，
  组合出逐 race 构造链抽数，与 Rust 实现逐 race 对齐 ⇒ **MISMATCH=0**（race 96 的条件支单列：命中 10 / 未命中 9）。
- **改坏必红 5 条**（`30/31/32/42/43-redcheck-*.txt`，均 rc=101）：RC-1 删 race-51 内联抽、RC-2 删 Dir 抽、
  RC-3 给 `spawn_monster_precomputed` 加取数、**RC-4 删构造链公共 `draw_base_and_animal`（F-1 专测）**、
  **RC-5 交换 BaseObject 两抽次序（只被值判据捕捉）**；每次还原后 sha256 与备份逐字节一致。
- 四门禁：`fmt rc=0`；`clippy rc=0`；`msgcodegen --check rc=0`（604）；
  `test --workspace --offline rc=0`（**86 suites / 751 passed / 0 failed / 18 ignored**）。
- 边界：`git diff --stat master -- src/ sql/ MirClient` **空**（`20-boundary.txt`）。

## 交付基线（base = master `670a009b`）

本 patch 的 base 是 **master `670a009b`**（已含第 ② 片 + R5-1/R5-2 返工；本次交付前已 `git merge origin/master`
消除幻影差量），head 是 `refs/heads/mongen-host`。
⇒ `git diff master..HEAD` **只含第 ③ 片**：`bins/game-svr/{mongen.rs,lib.rs,config.rs,main.rs}` +
`crates/world/src/{world.rs,entity.rs,lib.rs}` + 本目录证据。R5 的口径见已合入的
`tests/parity/world/mongen-20261013/README.md` §8.1（**不在本 patch 差量内**）。

> 本次 head 相对上一被审 head `3144315a` 的**增量** = F-1 返工（构造链取数 + 构造抽值落实体 + 全链判据
> + 证据措辞订正）+ M-1 订正；分片①②不重审。

## 未完成 / 下一片

- **真机两侧 100 杀窗口**：按卡 DoD⑤ 等 owner 发放（依赖承接卡 `openmir2-m3-monitems-attach`）。
- **实体行为面**：`ChickenDeer`/`AtMonster`/守卫/门的 AI 与 `dir`/`cool_eye` 落地属承接卡
  `openmir2-m3-monster-ai-tick`（本片只保证取数次序与基础实体能力）。
- **20ms 节拍**：本宿主按 200ms tick 驱动；若 Claim B 的 site/order 账本要求逐 20ms
  interleaving，需要在承接卡/Claim B 卡里评估（本片按设计文档的单线程 tick 口径）。
