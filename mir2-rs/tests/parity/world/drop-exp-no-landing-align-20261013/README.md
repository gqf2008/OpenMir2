# M3④-D 掉落侧「对齐冻结 `src/` 不落地」片（2026-10-13）

> 卡：**`openmir2-m3-drop-exp-wiring`**（issue `58825bcc`，M3④-实现 D）。
> 本片 = owner `aace16e0` / brief 顶部 09:35 裁决的**方案 A**：把 Rust 死亡掉落从「落地」
> 改回**冻结 `src/` 的真实行为（恒不落地）**，并修正 C# golden 探针 + 重生成 2 份 golden
> + 收窄白名单 T-32。
> 工作目录 `.claude/worktrees/m2-svr`（分支 `drop-exp-host`）；base = `origin/master` **`d778ac30`**
> （交付前 `git merge origin/master`，merge 提交 `d6aae2e0`）；无栈、无端口（判据全离线）。

## 0. 一句话

冻结 oracle 的死亡掉落（物品 + 金币）**恒不落地**（`Envir.AddItemToMap` 首行 `MapItem.ItemId == 0` 门，
全仓无赋值点）；旧 golden 编码的是**探针意图**（C# 探针用桩 `AddMapObject` 绕过该门）。
本片让 **Rust 生产路径、C# 探针、golden 三方**统一到冻结 `src/` 行为：不落地、`SM_ITEMSHOW(610)` 恒不发。

## 1. oracle 锚点（冻结 `src/`，见 `01-oracle-anchors.txt`）

| # | 位置 | 事实 |
| --- | --- | --- |
| A1 | `rg 'ItemId\s*=' src` | 唯一赋值点在 `src/Modules/SystemModule/Data/MapItem.cs:41`，且**是注释**（`// this.ItemId = …`）；其余 4 处全是 `ItemId == 0` 守卫（`PlayObject.cs:3155`、`Envirnoment.cs:90/120`、`RobotPlayer.Base.cs:547`） |
| A2 | `src/M2Server/Maps/Envirnoment.cs:88-93` | `AddItemToMap` **首行** `if (mapItem.ItemId == 0) return false;` |
| A3 | `src/M2Server/Actor/BaseObject.cs:522-580/730-768` | `DropItemDown`/`DropGoldDown` 造 `MapItem{…}`（无 ItemId）→ `GetDropPosition` → `AddItemToMap`；**成功支**才 `SendRefMsg(RM_ITEMSHOW,…)` |

⇒ 死亡掉落恒不落地、`RM_ITEMSHOW`/`SM_ITEMSHOW(610)` 恒不执行（同 M3① 的 T-19/T-25 根因）。
Rust 侧玩家 `CM_DROPITEM` 早已按 T-19 复刻（`bins/game-svr/src/ground.rs:264/609`），
本片把**死亡掉落**这条路径收敛到同一口径，消除「Rust 自身两处不一致 + 与 oracle 不一致」。

## 2. 改了什么（改前 → 改后）

| 文件 | 改前 | 改后 |
| --- | --- | --- |
| `crates/world/src/world.rs::settle_deaths` | 逐件 `place_floor_item` 落格、`placed` = 落地件数 | **不落格**、`placed` 恒 0；掉落**生成**（`MonGetRandomItems` 预算）不变 |
| `crates/world/src/world.rs` | `fn place_floor_item`（`GetDropPosition` 扫描 + `add_floor_item`） | **删除**（生产无调用点） |
| `crates/world/src/world.rs` 2 条用例 | `monster_death_drops_items_and_awards_exp` 断言 `placed > 0`；`monster_death_floor_item_view_matches_world_state` 从落地物构造视图 | 均改为「不落地」语义；新用例 `monster_death_drops_do_not_land_matches_oracle_add_item_to_map_gate` 同时钉住**掉落生成仍在**（金币预算 > 0）与**只读分片机制仍正确**（显式落格注入 ⇒ 视图与格子逐项一致） |
| `bins/game-svr/src/main.rs::tests::pickup_gold_dispatches_and_updates_session` | 用击杀链产金币落地（依赖已取消的落地） | 改**显式落格注入**（`maps[0].add_floor_item(...)`），保留**可达面**判据（位置门 `CurrX==nParam2 && CurrY==nParam3`、拾取成功回 `SM_GOLDCHANGED`） |
| `tests/parity/csharp/WorldGolden/Program.cs` | 死亡结算用**桩** `AddMapObject` 绕过 `AddItemToMap` 门 | 按 oracle 建模：仍走 `GetDropPosition` 扫描（无副作用），**不加进地图**、`placed` 恒 0（见 `02-golden-probe-before-after.txt`） |
| `tests/parity/world/golden/{combat,session}.txt` | `death` 行 `placed` = 1/2 + 若干 `floor` 行 | **重生成**：`death` 行 `placed` = 0、`floor` 行全消失（见 `04-golden-before-after-diff.txt` / `05-golden-hashes.txt`） |
| `tests/parity/tests/world_session_parity.rs` | `assert!(mine.contains("floor "))` + 红检按「地面物品数」 | 断言**无** `floor` 行；红检改为「改掉必掉率 ⇒ `death` 行金币字段必变」（掉落生成仍可见） |
| `tests/parity/whitelist.md` T-32 | 「帧未发 + `AddBodyLuck`/升级 `IncHealthSpell` 未实现」 | 收窄：删除已实现的假条目，只留**真缺口**（组队 `GainExp`、墙钟 30s/6s 过期）；`SM_ITEMSHOW(610)` 改为「oracle 不可达」（见 `12-whitelist-t32-after.txt`） |
| `crates/world-bevy/tests/bevy_drop_parity.rs`（**跨写集，见 §5**） | 断言 2 从 golden 的 `floor` 行构造期望 | 断言 golden **无** `floor` 行 + `plan_floor_placement` 改用 oracle 扫描序期望值钉住 |

## 3. 改坏必红（见 `06/07/08/09-*.txt`）

1. **`settle_deaths` 改回落地**（临时用 `d2155f9c` 的 `world.rs`）⇒
   `world_combat_parity::combat_attacks_and_exp_match_csharp` **RED**：
   `rust = death 2 1 192 1 100` vs `csharp = death 2 0 192 1 100`（首个差异第 36 行）；
   `world_session_parity::session_and_drops_match_csharp` **RED**：
   `rust = death 2 2 192 -1 0` vs `csharp = death 2 0 192 -1 0`（第 18 行）。
2. **C# 探针改回桩 `AddMapObject`**（绕过门）⇒ 重产 golden 含 48 条 `floor` 行 ⇒
   `world_combat_parity` **RED**：`rust = death 2 0 …` vs `csharp = death 2 1 …`。
   ⇒ 证明「探针保真」是 load-bearing 的（golden 与冻结 `src/` 必须一致）。
3. 还原后 `world_combat_parity` 7/7、`world_session_parity` 2/2、`bevy_drop_parity` 4/4 **GREEN**。

## 4. 门禁与证据

- 四门禁（`CARGO_TARGET_DIR=E:/tmp/m2svr-optionA-target`，`--offline`，在 **merge `origin/master` `d778ac30` 后的 head `d6aae2e0`** 上重跑）：
  `fmt` rc=0；`clippy --workspace --all-targets -D warnings` rc=0；`test --workspace --no-fail-fast`
  **93 suites · 847 passed · 0 failed · 20 ignored** rc=0；`msgcodegen --check` **604** rc=0（见 `10-gates.txt` + `gates/`）。
  （merge 前 base `083dd0e3` 树读数为 92/837/0/19；差量 = merge 带入的 master 新代码，0 失败。）
- 边界：`git diff --name-only origin/master..HEAD -- src sql MirClient` **空**（见 `11-boundary.txt`）。
- 证据清单（仓库字节 / LF 归一口径）由 `tools/evidence-hash/verify_sha256sums.py` 生成并复核。

## 5. 跨写集说明（请 owner/审查注意）

- `crates/world-bevy/tests/bevy_drop_parity.rs` 的断言 2 是 golden `floor` 行的**直接消费方**；
  golden 语义改为「不落地」后该断言的前提失效。本片只改**该测试**（断言 golden 无 floor 行 +
  用 oracle 扫描序的显式期望值钉住 `plan_floor_placement`），**未改 `crates/world-bevy` 的生产代码**
  （`src/lib.rs`/`drop_bench` 不动）。wbevy 车道已 park、无写集冲突；如 owner 认为应改由 wbevy 车道收口，
  可在合并前转派。
- 若日后 owner 决定「修 oracle（给 `MapItem` 补 `ItemId` 赋值）」，需把 Rust/golden/探针这四处
  一并恢复为「落地」——本片已把该口径钉在 golden + 用例里，改回去必红。

## 6. 残留 / 未核项（如实登记）

- **真机 100 杀（DoD②③⑤）仍要独占窗口**：本片只做离线/构造层，未起栈、未占端口（十端口与开工前一致，仅共享 3306）。
- 白名单 **T-32 收窄后仍需独立 principal 复核**（本片只改文案与范围，收敛随 `openmir2-m3-exp-misc` 与墙钟过期承接）。
- `E:/tmp/m2svr-optionA/`（重产 golden、改坏必红原始件、Program.cs/world.rs 备份）、
  `E:/tmp/m2svr-optionA-target/`（隔离 cargo target）为本片仓外临时物，可删。
- 未 merge、未自签 review。
