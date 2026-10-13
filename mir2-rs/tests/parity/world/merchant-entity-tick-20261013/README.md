# M3⑤-实现 2 第①片：世界内商人实体 + `FindMap` 门挂图 + `rename_rand`（2026-10-13）

> 卡：**`openmir2-m3-merchant-entity-tick`**（M3⑤-实现 2；owner 归属裁决 = `26efc51d`「写集
> `crates/world/**` + `bins/game-svr/**`（m2svr 单线）」，lanes.conf 10:05「⑤-2 世界实体线归 m2svr 单线」）。
> 工作目录 `.claude/worktrees/m2-svr`（分支 `merchant-entity-host`）；
> base = `master` **`171741498fadc1c0667acdbaf32241fb551e5d7c`**（已含 ④-D `4521d451`）。
> 本片为**第①片**：实体 + 挂图；`Merchant.Run`/`RefillGoods`/`ProcessMerchants` tick 留给后续分片（登记）。

## 0. 一句话

把 ⑤-1 的 `crates/envir` 商人记录接到世界实体：新增 `crates/world/src/merchant.rs`
（`MerchantState` + `World::spawn_merchant`，复刻 `BaseObject.Initialize` 的
`AddtoMapSuccess` 语义），宿主 `bins/game-svr/src/merchant.rs` 做装载 + **O-1 `FindMap` 门** +
**O-2 `rename_rand` 注入**，`sim_loop` 起世界后逐条挂图。

## 1. 交付内容

| 文件 | 内容 |
| --- | --- |
| `crates/world/src/merchant.rs`（新） | `MerchantState`/`MerchantFlags`/`MerchantGoods`/`MerchantSpawn`；`World::spawn_merchant`（`Initialize`/`AddtoMapSuccess`：`!(CanWalk && AddToMap)`，为真且 `!IsHide` ⇒ 移除，`WorldServer.Merchant.cs:8-25`）；`merchant_ids_sorted`；`std_item_idx_by_name`/`std_item_by_idx`（`GetStdItemIdx`/`GetStdItem` 口径） |
| `crates/world/src/entity.rs` | `Entity.merchant: Option<MerchantState>`（新字段，缺省 `None`） |
| `crates/world/src/monai.rs` | `monster_ai_tick` 显式跳过 `merchant.is_some()`（`ActorRace.Merchant == Animal == 50`，靠 race 会误跑 `monster_run`） |
| `crates/world/src/lib.rs` | `pub mod merchant;` + re-export |
| `bins/game-svr/src/merchant.rs`（新） | `load_merchant_spawns`（`load_merchants` + **O-1 门** + **O-2 `rename_rand`**）与 `spawn_merchants`；`MerchantLoadReport` |
| `bins/game-svr/src/lib.rs` | `pub mod merchant;` |
| `bins/game-svr/src/main.rs` | 造 `MapLoader` 传进 `EnterConfig.loader`；`sim_loop` 起世界（`world.rng` 就位）后装载 + 挂图并打读数日志 |

## 2. ⚠ owner `26efc51d` 的 O-1 前提经本片实测**不成立**（附锚）

owner 说 4 条记录（`DB`×1 / `ZM6`×1 / `jiaren`×2）的 map 名在 `Envir/MapInfo.txt` 与 `Map/*.map`
里都没有 ⇒ 应丢到 127。**实读**（`02-real-deployment-vector.txt`）：

- `Envir/MapInfo.txt:1 [jiaren|0150 …]`、`:21 [ZM6|D5071 …]`、`:10 [DB|DM011 …]` —— 三段都在；
- `Map/0150.map` / `Map/D5071.map` / `Map/DM011.map` —— 三个物理文件都在（`AddMapInfo` 落表条件）；
- oracle `FindMap`（`MapManager.cs:146`）按**逻辑名**（`|` 前）查 `_mapList`，**不是** `.map` 文件名
  ⇒ 4 条全部命中。

⇒ 真机向量 = **`records=131 kept=131 dropped=[]`**。owner 疑似把**逻辑名**当成 `Map/*.map` 的
**文件名**去找。**`FindMap` 门仍按 oracle 实现**（`resolve_map` 未命中即丢），本部署只是恰好不触发；
门的可用判据是合成用例 `find_map_gate_drops_unknown_map`（`05-redcheck-findmap-gate.txt`）。
**请 owner 据此修订卡上的 O-1 判据文案。**

## 3. 判据与改坏必红

- 合成用例（`crates/world` 3 条 + `bins/game-svr` 3 条 + 1 `#[ignore]`）：
  `find_map_gate_drops_unknown_map`（`records=3 kept=2`）、`gated_spawns_attach_to_world`、
  `missing_merchant_txt_is_empty`、`merchant_attaches_to_walkable_cell_and_is_listed`、
  `merchant_dropped_when_cell_not_walkable`、`std_item_lookup_is_one_based_and_case_insensitive`。
- **改坏必红**：去掉 O-1 门（`None` 支照收 `map_id=0`）⇒ `find_map_gate_drops_unknown_map`
  `FAILED`（`left: 3` vs `right: 2`）；还原后 3 passed / 1 ignored。
- 真机向量：`records=131 kept=131 dropped=[]`（`#[ignore]`，需 `E:/MirServer/M2GameSvr`）。

## 4. 四门禁 / 边界 / 窗口

- 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-merchant-target`，`--offline`）：
  fmt rc=0；clippy `--workspace --all-targets -D warnings` rc=0；msgcodegen rc=0（604）；
  `test --workspace --no-fail-fast` **93 suites · 853 passed · 0 failed · 21 ignored**
  （base master 树 847/0/20 ⇒ +6 passed +1 ignored）。
- 边界：`git diff --name-only master..HEAD -- src sql MirClient` **空**。
- **未起栈 / 未占端口**（全离线判据；真机对拍要窗口，登记等 owner）。

## 5. 未建模（登记，本卡后续分片）

`Merchant.Run`（`Merchant.cs:112-170`）与 `RefillGoods`（`:209-276`）、`ProcessMerchants` 分片 tick
（`WorldServer.Merchant.cs:65-110`，`M2Share.NpcLimit=5ms` 的确定性等价物待 owner 拍板）、构造期
`Random(1500)`（`BaseObject.cs:439`，避免顺序落错）、`ProcessRefillIndex`（全局 `CurrentMerchantIndex++`，
⑤-1 未随记录暴露）、`GoodsList`/`ItemPriceList` 库存与 `LocalDB.SaveGoodRecord` 持久化（接缝 ⑤-5）。

## 6. 证据清单

`00-commands.txt` / `01-oracle-anchors.txt` / `02-real-deployment-vector.txt` /
`03-workspace-test.txt` / `04-gates-fmt-clippy-msgcodegen.txt` /
`05-redcheck-findmap-gate.txt` / `06-boundary.txt` / `SHA256SUMS.txt` / `README.md`。
