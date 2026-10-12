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
| `crates/world/src/world.rs` | 新增 `World::budget_monster_drops`（用调用方给定的流做掉落预算）与 `World::spawn_monster_precomputed`（预置掉落/朝向，**不再取随机数**）+ 2 条用例 |

`CreateMonster` 回调的取数次序（`WorldServer.MonGen.cs:498-810`）：
`FindMap==null ⇒ None`（不抽）→ race 不在 switch ⇒ `None`（不抽）→ per-race 抽（51/52/53/95/96/97）
→ `Dir = RandomByte(8)` → `MonGetRandomItems`（掉落预算）→ `Random(100) < CoolEyeCode` → 地形移位。

## 关键口径

- **单条随机流贯穿**：`step_mon_gen` 把 `world.rng` 临时移出交给 `MonGenHost::step`，
  `WorldMonsterSink` 只落地、不再取数；回调在两次位置抽之间同步完成 `CreateMonster` 的取数
  ⇒ 与 oracle 的 interleaving 逐调用相同（`regen_monsters` 的回调契约）。
- **RNG 位置判据**：每个 race 分支的用例都用「第二个同种子流手工复算 oracle 次序」对照，
  比较函数返回后的**下一抽**——改错抽数个数/次序即红（见 red-check RC-1/RC-2）。
- **登记差异**：oracle 每线程 `Thread.Sleep(20)`，本宿主每 200ms world tick 驱动一次
  （设计文档 §4.2 硬约束①单线程确定性）；`dir`/`cool_eye` 已取数但 world `Entity` 未建模
  （AOI 面属承接卡 `openmir2-m3-monster-ai-tick`）。

## 判据与门禁（全绿）

- `cargo test -p game-svr --lib mongen`：**12 passed**（RNG 次序 ×6、表/转换 ×2、配置解析 ×1、
  宿主 start/step ×3）。
- `cargo test -p mir2-world --lib`：41 passed（含本片新增 2 条；第 ② 片 R5 的 3 条也在此计数内）。
- **改坏必红 3 条**（`30/31/32-redcheck-*.txt`，rc=101）：RC-1 删 race-51 抽数、RC-2 删 Dir 抽数、
  RC-3 给 `spawn_monster_precomputed` 加取数；还原后 sha256 与备份逐字节一致。
- 四门禁：`fmt rc=0`；`clippy rc=0`；`msgcodegen --check rc=0`（604）；
  `test --workspace --offline rc=0`（**86 suites / 749 passed / 0 failed / 18 ignored**）。
- 边界：`git diff --stat master -- src/ sql/ MirClient` **空**（`20-boundary.txt`）。

## 交付基线（base = master `304126ae`）

本 patch 的 base 是 **master `304126ae`**（已含第 ② 片 + R5-1/R5-2 返工），head 是 `refs/heads/mongen-host`。
⇒ `git diff master..HEAD` **只含第 ③ 片**：`bins/game-svr/{mongen.rs,lib.rs,config.rs,main.rs}` +
`crates/world/src/world.rs` + 本目录证据。R5 的口径见已合入的
`tests/parity/world/mongen-20261013/README.md` §8.1（**不在本 patch 差量内**）。

## 未完成 / 下一片

- **真机两侧 100 杀窗口**：按卡 DoD⑤ 等 owner 发放（依赖承接卡 `openmir2-m3-monitems-attach`）。
- **实体行为面**：`ChickenDeer`/`AtMonster`/守卫/门的 AI 与 `dir`/`cool_eye` 落地属承接卡
  `openmir2-m3-monster-ai-tick`（本片只保证取数次序与基础实体能力）。
- **20ms 节拍**：本宿主按 200ms tick 驱动；若 Claim B 的 site/order 账本要求逐 20ms
  interleaving，需要在承接卡/Claim B 卡里评估（本片按设计文档的单线程 tick 口径）。
