# M3④-实现 D 分片 D①：击杀经验链（`ExpHitter.Master` 归属 + 地图 `EXPRATE` + `SM_WINEXP(44)` + 会话同步）

> 卡：`openmir2-m3-drop-exp-wiring`（issue `58825bcc`，父级 `openmir2-m3-drop-exp-align` 十项之一）。
> 本片只做 **经验链** 这一段；**掉落落地 / `SM_ITEMSHOW(610)`** 见 §5「未做（D②）」。
> base = `origin/master` **`b7fef4a7`**；head = 分支 `drop-exp-host`（本片单提交）。
> 工作目录 `<worktree>/mir2-rs`；隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-d1-target`。

## 0. 一句话

`world` 侧原来只把击杀经验给**玩家本人**；本片按 oracle `KillFunc` 补齐 **`ExpHitter.Master`（宠物/下属击杀给主人）** 支，
补 **`Envir.Flag.EXPRATE` 地图倍率**，把归属结果经 `TickRecord::exp_awards` 交给宿主，
宿主发 `SM_WINEXP(44)` 并把世界实体的**经验/等级同步回会话**（否则存档会丢本 tick 的经验）。

## 1. oracle 锚点（原文见 `01-oracle-anchors.txt`）

| 步骤 | 行号 |
| --- | --- |
| `Die()` → `KillFunc()` | `BaseObject.Base.cs:318/:343` |
| **`KillFunc` 的归属两支** | `BaseObject.cs:2429-2444`：`ExpHitter.Master != null` ⇒ `RM_PLAYERKILLMONSTER` 给 master（`:2434-2437`）；`ExpHitter.Race == Play` ⇒ 再给本人（`:2441-2444`） |
| `RM_PLAYERKILLMONSTER` → `KillTargetTrigger` | `PlayObject.cs:1632`：`monsterExp = CalcGetExp(WAbil.Level, fightExp)`（`:1643`）→ `GainExp(monsterExp)`（`:1652`） |
| `CalcGetExp` | `BaseObject.cs:712` |
| `GainExp`（组队分支） | `PlayObject.Base.cs:764`（单人 `:810/:815` → `WinExp`） |
| `WinExp`（`KillMonExpMultiple`/`MNKillMonExpMultiple`/`KillMonExpRate`/**`Envir.Flag.EXPRATE`**） | `PlayObject.Base.cs:129`：地图倍率在 `:143-146` |
| `GetExp`（加经验/单步升级/帧） | `PlayObject.Base.cs:149`：`Abil.Exp += dwExp`（`:151`）、`SendMsg(RM_WINEXP,0,dwExp,0,0)`（`:153`） |
| `RM_WINEXP → SM_WINEXP` 编码 | `PlayObject.Message.cs:1666-1669`：`MakeMessage(SM_WINEXP, Abil.Exp, LoWord(nParam1), HiWord(nParam1), 0)` + `SendSocket` ⇒ `Recog=总量`、`Param/Tag=增量的 Lo/Hi`、无尾 |

## 2. 本片实现（写集 `crates/world/**` + `bins/game-svr/**`）

- `crates/world/src/combat.rs`：`award_kill_exp` 新增 `map_exp_rate: Option<i32>`，在 `KillMonExpRate` 之后、
  `GetExp` 之前按 `Envir.Flag.EXPRATE`（`Round(rate/100*dwExp)`）加倍（`None` ⇒ 一字不改）。
- `crates/world/src/world.rs`：
  - `World::map_exp_rate: Vec<Option<i32>>` + `World::set_map_exp_rate`（缺省全 `None`）；
  - `TickRecord::exp_awards: Vec<(获得者 id, 本次获得, 是否升级, 升级后等级)>`；
  - `settle_deaths` 按 `KillFunc` 两支算归属：`ExpHitter.Master`（有）与 `ExpHitter`（`Race==Play`）**各自**结算，
    并写 `exp_awards`；`deaths` 的 5 元组口径仍取「主获得者 = master 优先」。
- `bins/game-svr/src/smframe.rs`：新增 `sm_winexp(total_exp, delta)`（头 = `command_header(total_exp, SM_WINEXP, LoWord(delta), HiWord(delta), 0)` + 无尾）。
- `bins/game-svr/src/main.rs`：
  - 起世界时按 `MapRegistry::flag_of(map_id)` 的 `bo_exprate`/`exp_rate` 逐图灌 `world.set_map_exp_rate`；
  - `drain_combat_downlink` 新增**经验帧**（③段）：按 `rec.exp_awards` 找到在线会话 ⇒ 同步
    `sg.equip.abil.exp/level/max_exp` 与 `sg.state.wabil`（世界实体是权威值）⇒ 发 `SM_WINEXP`。

## 3. 判据（新增 3 条用例，全部构造级）

| 用例 | 位置 | 钉住 |
| --- | --- | --- |
| `world::tests::pet_kill_awards_exp_to_master` | `crates/world/src/world.rs` | `ExpHitter.Master` 支：经验给主人；宠物（非 Play）不加；`deaths[0].4 == 增量` |
| `world::tests::map_exp_rate_multiplies_kill_exp` | `crates/world/src/world.rs` | `EXPRATE=200 ⇒ 单次经验 ×2`（无 EXPRATE ⇒ = `mon_exp`） |
| `tests::exp_downlink_sends_sm_winexp_and_syncs_session` | `bins/game-svr/src/main.rs` | 帧逐字节（`Recog=总量`/`Param=LoWord`/`Tag=HiWord`/无尾）+ 会话 `exp/level` 同步 |

## 4. 改坏必红（原始件 `40/41/42`，三文件 `43` 还原核对）

| 编号 | 变异 | 必红用例 | rc |
| --- | --- | --- | --- |
| RC-D1-A | 删掉 `settle_deaths` 的 `ExpHitter.Master` 支 | `pet_kill_awards_exp_to_master` | 101（`FAILED`） |
| RC-D1-B | 地图 EXPRATE 倍率改 `×0` | `map_exp_rate_multiplies_kill_exp` | 101（`FAILED`） |
| RC-D1-C | 交换 `sm_winexp` 的 `Param`/`Tag` | `exp_downlink_sends_sm_winexp_and_syncs_session` | 101（`FAILED`） |

还原后 `crates/world/src/{world,combat}.rs`、`bins/game-svr/src/smframe.rs` 的 sha256 与变异前**逐字节一致**（`43`）。

## 5. 未做（D② 候选，本片如实登记）

- **掉落落地 + `SM_ITEMSHOW(610)`**：**未做**，且见 §6 的**发现** —— oracle 侧该帧在本仓**不可达**。
- `GainExp` 的**组队分支**（`GroupOwner`/`GroupMembers`：`sumlv/n`、`bonus[1..12]`、按距离 12 过滤）：**未做**（world 无组模型）。
- `GetExp` 的 **`AddBodyLuck(dwExp*0.002)`**、升级 `AddBodyLuck(100)`、`IncHealthSpell(2000,2000)`：**未做**（`Entity` 无 `body_luck`/`mp`/`max_mp`）。
- `WinExp` 的 `LimitExpLevel`/`LimitExpValue` 支：**未做**（现网配置下不可达，登记）。
- `LastHiter`/`ExpHiter` 的**墙钟** 30s/6s 过期：**未做**（world 用虚拟时钟；`expire_hitters` 只清「命中者已死/已隐身」）。
- 升级帧 `SM_LEVELUP`/`SM_ABILITY`/`SM_SUBABILITY`（`RM_LEVELUP` 支）：**未做**（属 D②/装备面）。

## 6. 发现（交 owner 裁决，本片**不擅自改**）：Rust 死亡掉落会落地，oracle 恒不落地

- oracle：全仓**无** `MapItem.ItemId = …` 赋值点（`src/Modules/SystemModule/Data/MapItem.cs:41` 是注释），
  `Envirnoment.AddItemToMap` 首行 `if (mapItem.ItemId == 0) return false;`（`:88-92`）⇒
  `ScatterBagItems`/`DropGoldDown`/`DropItemDown` 造出的 `MapItem` **恒加不进地图** ⇒
  `DropItemDown` 成功支的 `SendRefMsg(RM_ITEMSHOW,…)`（`BaseObject.cs:570`/`:752`）**永不执行**。
  （M3① 的 T-19/T-25 已登记同根。）
- Rust：`world.rs::settle_deaths` → `place_floor_item` **会**把掉落写进 `MapGrid`（`deaths[].1 = placed > 0`），
  且 `bins/game-svr` 的拾取路可经 `pickwire::build_ground_map(floor_items_view_sorted())` 看到它。
- ⇒ 两侧在「地面物可达面」上**不同源**（oracle 不可达、Rust 可达）。本片**未动**该语义（既不删 Rust 落地，
  也不实现 oracle 不可达的 `SM_ITEMSHOW`），只如实登记 + 等 owner 裁决（**行为等价优先**：要不要把 Rust 落地
  也按 `ItemId==0` 复刻成「不落地」，会牵动 world golden T-31 与拾取面 T-25，属跨卡）。
- 记录：`SM_ITEMSHOW(610)` 的**编码**本片也未加（不可达即无判据；等 owner 裁决落地语义后再定）。

## 7. 复现

```bash
# 判据（3 条）
CARGO_TARGET_DIR=E:/tmp/m2svr-d1-target cargo test -p mir2-world --lib --offline pet_kill_awards_exp_to_master
CARGO_TARGET_DIR=E:/tmp/m2svr-d1-target cargo test -p mir2-world --lib --offline map_exp_rate_multiplies_kill_exp
CARGO_TARGET_DIR=E:/tmp/m2svr-d1-target cargo test -p game-svr --bin game-svr --offline exp_downlink

# 四门禁
CARGO_TARGET_DIR=E:/tmp/m2svr-d1-target cargo fmt --all -- --check
CARGO_TARGET_DIR=E:/tmp/m2svr-d1-target cargo clippy --workspace --all-targets --offline -- -D warnings
CARGO_TARGET_DIR=E:/tmp/m2svr-d1-target cargo run -q -p msgcodegen --offline -- --check
CARGO_TARGET_DIR=E:/tmp/m2svr-d1-target cargo test --workspace --offline --no-fail-fast

# 证据 hash（仓库 LF/git-blob 口径）
python -I tools/evidence-hash/verify_sha256sums.py write mir2-rs/tests/parity/world/drop-exp-wiring-20261013
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/world/drop-exp-wiring-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/world/drop-exp-wiring-20261013
```

## 8. 纪律

- 只动 `mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**`（本卡写集）；`src/` `sql/` `MirClient` **零改动**（`06-boundary.txt`）。
- 未起栈、未占端口、未连 MySQL（判据全部构造级）。
- 未 merge、未自签 review；交付 `patch` + 紧接 `status: needs-review`。
