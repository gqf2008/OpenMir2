# M3④-实现 C 第 ① 片 —— world 侧怪物实体 AI 核心（证据）

- 卡：`openmir2-m3-monster-ai-tick`（M3 逐系统对齐 ④ 的承接卡 C，依赖 B=`openmir2-m3-mongen-load-spawn`）
- 分支/head：见卡上 `patch`/`status` 条目（base = 交付时 `refs/heads/master`）
- 写集：`mir2-rs/crates/world/**`（本片）+ 本证据目录；**零** `src/ sql/ MirClient`

## 本片做了什么

把 oracle `AnimalObject`/`MonsterObject` 的**确定性 AI 子集**落到 `crates/world`：

| 文件 | 内容 |
| --- | --- |
| `crates/world/src/entity.rs` | 新增 `AnimalState`（逐字段 oracle 锚点见 doc comment）+ `Entity.ai` 字段 + `Entity::dir()` |
| `crates/world/src/monai.rs`（新） | `walk_to`/`turn_to`/`wondering`/`search_target`/`goto_target_xy`/`think`/`attack_target`/`can_move`/`monster_run`/`monster_ai_tick`/`get_attack_dir`/`count_at_cell` |
| `crates/world/src/world.rs` | `World::tick` 在死亡结算之后、AOI 之前按**显式开关**驱动 `monster_ai_tick`；`attack_once` 升 `pub(crate)` |
| `crates/world/src/lib.rs` | `pub mod monai;` + 重导出 |

**关键设计（缺省关闭）**：`World` 新增私有的 `ai_enabled`（缺省 `false`），`enable_monster_ai()` 开启，
`monster_ai_enabled()` 读取。理由：`tests/parity/world/scenarios/*.json` 的 golden 是
`csharp/WorldGolden` 的**合成探针**（直接驱动 `_Attack`，不含 oracle 的 `ProcessMonsters` 线程），
默认跑 AI 会把这些 golden 的 RNG 序列整体移位 —— 实测（未加开关时）`world_combat_parity`
的 `atk 1 1 20` vs golden `24` 即此因。生产接线由 `bins/game-svr` 的 sim 循环显式开启
（**宿主接线 = 本卡后续分片**）。

## 判据

- 构造级 **8 条**用例（`04-tests-monai.txt`）：`Wondering` 三条取数次序/语义、`SearchTarget`
  选最近并清无效、`GotoTargetXY` 的 `Random(3)` 次序、`Think` 的 `DupMode` 门、
  `monster_ai_tick` 的槽位序/确定性、**AI 缺省关闭**。
- **改坏必红 3 条**（`redchecks/`）。
- 既有 golden 回归：`world_combat_parity` 7/7、`world_session_parity` 2/2 全绿（AI 缺省关闭 ⇒ 逐字节不变）。
- 四门禁：`gates/`（fmt / clippy -D warnings / msgcodegen 604 / workspace 86 suites·743 passed·0 failed·18 ignored）。

## 关键 finding（交 owner 裁决）

**构造期 RNG 缺口**：oracle `new MonsterObject()` 在 `CreateMonster` 的 `cert.Dir = RandomByte(8)`
**之前**先抽 5 次（BaseObject 2 + AnimalObject 2 + MonsterObject 1），而 `crates/world/src/mongen.rs`
的 `MonsterCreator` 契约只声明了 Dir/掉落/CoolEye 三抽 ⇒ 真机 100 杀的 RNG 序列会整体提前。
详见 [`02-constructor-rng-gap.txt`](02-constructor-rng-gap.txt)。本片**未单方面改动**被审的
B 代码，只在 `Entity.ai` 预留了对应字段。

## 复现命令

```sh
cd mir2-rs
CARGO_TARGET_DIR=E:/tmp/<tag>-target cargo test -p mir2-world --lib --offline monai
CARGO_TARGET_DIR=E:/tmp/<tag>-target cargo test --workspace --offline --no-fail-fast
```

证据清单 `SHA256SUMS.txt` 按仓库 LF/git-blob 口径；复核：
`python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/world/monster-ai-20261013`
