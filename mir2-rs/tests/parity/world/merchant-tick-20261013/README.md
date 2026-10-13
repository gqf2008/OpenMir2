# M3⑤-实现 2 第②片：`ProcessMerchants` tick + `Merchant.Run`/`RefillGoods` + `merchant` 组 RNG 站点

> 卡：**`openmir2-m3-merchant-entity-tick`**（M3⑤-实现 2）。owner 归属裁决 = `26efc51d`
> （写集 `crates/world/**` + `bins/game-svr/**`，m2svr 单线）；本片 DoD = owner `eb3bf42f`
> 的 ⑤ 条判据 + `b311a876b1` 转交的 findings。
> 工作目录 `.claude/worktrees/m2-svr`（分支 `merchant-entity-host`）；
> base = `refs/heads/master`（`0dde01b7`，已含本卡**第①片** `85dafe63`）。

## 0. 一句话

把 oracle 的商人 **200ms 定时线程**（`MerchantProcessor`）在单线程确定性 `World::tick` 里
等价落地：新增 `World::merchant_tick`（节奏门 + `RunTick/RunTime` 门 + Ghost 60s sim 时钟 +
**丢掉** `NpcLimit` 墙钟预算）、`merchant_run`（`Merchant.Run` 的 RNG/显隐/`MapRandomMove`）
与 `refill_goods` 计时态；构造抽 `RunTick = now + Random(1500)` 按 oracle 的**两个独立次序**落定
（正序构造抽 + 正序建实体 → 倒序挂图；见 §9）；RNG 站点全部落
`crates/world/src/merchant.rs` ⇒ Claim B 的 `merchant` 组。

## 1. 交付内容

| 文件 | 内容 |
| --- | --- |
| `crates/world/src/merchant.rs` | `MERCHANT_PASS_INTERVAL_MS=200`/`MERCHANT_GHOST_EXPIRE_MS=60000`/`MERCHANT_RUN_DELAY_MAX=1500`…；`MerchantState.castle_under_war`；`MerchantGoods.refill_tick`；`World::merchant_ctor_run_delay`（构造抽，正序）+ `World::insert_merchant`（建实体，正序）/ `World::attach_merchant_to_map`（挂图，倒序）**两段拆分**（reviewer-5 F-1）+ `World::merchant_tick`（`ProcessMerchants` 等价物）+ `merchant_run`/`refill_goods`/`map_random_move`；`merchant_rename_rand`（站点归本文件）；**唯一 id 修复**（`EntityStore::alloc_id`） |
| `crates/world/src/world.rs` | `merchant_enabled`/`merchant_position`/`merchant_pass_tick`；`enable_merchant_tick`；`TickRecord` 增 `merchants_run`/`merchant_turns`/`merchant_hits`/`merchant_space_moves`/`merchant_removed`；`tick()` 在**怪物 AI 之后、AOI 之前**驱动 |
| `bins/game-svr/src/merchant.rs` | 模块头/`MerchantLoadReport` doc 订正（M-1：131→131）；`spawn_merchants` **三步骤两序解耦**（正序抽 + 正序 `insert_merchant` + 倒序 `attach_merchant_to_map`；reviewer-5 `0f220cac` F-1）；`rename_rand` 经 `merchant_rename_rand` 转发 |
| `bins/game-svr/src/main.rs` | 生产 `sim_loop` `world.enable_merchant_tick()` + 日志 |
| `bins/game-svr/tests/merchant_rng_trace.rs` | 独立测试二进制：真实 Rust trace 自证站点落 `crates/world/src/merchant.rs` |
| `tests/parity/whitelist.md` | T-33 doc-drift 订正 + 新增 **T-42**（丢 `NpcLimit` 墙钟预算，owner `eb3bf42f`） |
| `tests/parity/m3/drop-exp/drop_exp_consumers.tsv` | 新增 `rust wallclock` 的 `merchant` 组行（owner `eb3bf42f` ⑤） |

## 2. owner `eb3bf42f` 的 ⑤ 条判据（逐条落点）

| # | 判据 | 落点 / 证据 |
| --- | --- | --- |
| ① | 200ms 节奏门：同一窗口 `Run` 只一次 | `merchant_pass_cadence_blocks_second_pass_in_same_window`；红检见 `05`（去门 ⇒ FAILED；含 256 条规模版） |
| ② | `RunTime` 门：未到期不 Run、到期才 Run | `merchant_run_time_gate_not_due_then_due`；红检（`>RunTime`→`true` ⇒ FAILED） |
| ③ | Ghost 60s 移除用 **sim 时钟** | `merchant_ghost_expires_after_60s_sim_clock`；红检（60000→120000 ⇒ FAILED） |
| ④ | Claim B 合成件：两个商人次序对调 ⇒ `groupdiff` 必红 | `04-claimb-groupdiff.txt`（`SHARED_REL_ORDER_DIFF` rc=1；缺组 `GROUP_SET_DIFF` rc=1） |
| ⑤ | `NpcLimit` 登记为 wallclock consumer + 纯度门同步 | `whitelist.md` T-42 + `drop_exp_consumers.tsv` `rust wallclock` 行 + 纯度门 selftest 16/16 |

## 3. R-4（Claim B `merchant` 组）

- 真实 Rust trace（`00-rust-rng-trace.tsv`，`MIR2_RNG_LOG` 同口径）4 条站点全落
  `crates\world\src\merchant.rs:{246,450,508,514}`；`rng_ledger_diff.py groups --side rust
  --require-closed` ⇒ **`GROUP 4 merchant` + `GROUP_COVERAGE closed` + `ROW_COVERAGE closed`，rc=0**
  （`03-merchant-rng-trace.txt`）。
- 站点表 `consumer_groups.tsv` 的 `merchant|rust` 行已覆盖（本片**未改**该表 ⇒ 零表漂移）；
  `rename_rand` 经 `merchant_rename_rand` 归本文件，宿主不再产生未归类站点。
- 真机同场景两侧 `merchant` 组计数/次序对拍（窗口与 `openmir2-m3-rng-trace-scene-groups` 同批）
  仍等 owner 发窗口；本片登记为窗口项。

## 4. owner `b311a876b1` 转交 findings（逐条）

- **ruling_reverse_order**：`spawn_merchants` 拆成**两个独立次序**（reviewer-5 `0f220cac`
  F-1 返工）：正序构造抽 + **正序 `insert_merchant` 建实体**（槽位序 = 文件序 = `MerchantList`
  序 / tick pass 内序）+ **倒序 `attach_merchant_to_map` 挂图**（`obj_list` 追加 = 反序）。
  用例 `spawn_merchants_decouples_tick_order_from_obj_list_order` 同时钉住
  `merchant_ids_sorted()` == 文件正序 与 `obj_list[0]` = 后加载者（整体 `rev()` ⇒ 前者必红；
  挂图改回正序 ⇒ 后者必红）。
- **M-1**：`bins/game-svr/src/merchant.rs` 模块头 + `MerchantLoadReport` doc 订正为
  「131→131（门不触发）/`dropped=[]`」。
- **M-2**：新增 `monster_ai_tick_skips_merchants_race_is_animal`
  （删 `merchant.is_none()` 守卫 ⇒ 必红）。
- **M-3**：`crates/world/src/merchant.rs` 模块头点明 `is_hide` 不可注入、oracle 时序下 1:1。
- **附带缺陷修复**：`World::spawn_merchant` 原先 `Entity::new(0,..)` 未分配唯一 id ⇒ 多商人
  `id=0` 碰撞、`merchant_ids_sorted` 返回重复 0、tick 会反复 Run 同一实体。改用
  `EntityStore::alloc_id()`；`monster_ai_tick` M-2 用例与反序用例共同钉住。

## 5. 改坏必红（9 条，原始读数见 `05-redchecks.txt`）

节奏门（单条 + 256 条规模版）/ `RunTime` 门 / Ghost 窗口 / `Merchant.Run` 的 RNG 取数 / 唯一 id /
`monster_ai_tick` 的 `merchant.is_none()` 守卫 / **F-1a（spawn 循环整体 `rev()` ⇒ tick 序被反）** /
**F-1b（挂图改回正序 ⇒ `obj_list` 序被反）** ——
每条「改坏 ⇒ 目标用例 FAILED，`git checkout` 还原 ⇒ ok」。

## 6. 四门禁 / 边界 / 窗口

- 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-target`，`--offline`）：fmt rc=0；
  clippy `--workspace --all-targets -D warnings` rc=0；msgcodegen rc=0（604）；
  `test --workspace --no-fail-fast` **94 suites · 864 passed · 0 failed · 21 ignored**
  （见 `06-gates.txt` / `06b-workspace-test.txt`）。
- 边界：`git diff --name-only master..HEAD -- src sql MirClient` **空**。
- **未起栈 / 未占端口**（全离线判据；真机两侧对拍要窗口，登记等 owner）。

## 7. 未建模（登记，接缝）

- `GoodsList`/`ItemPriceList` 库存与 `SaveGoodRecord` 持久化 ⇒ ⑤-5；`refill_goods` 因此恒走
  `goods.Count > 0` 分支（一次 Run 最多补一件，oracle quirk 照搬）。
- `ClearExpreUpgradeListData`（墙钟 `DateTime.Now` + 升级表持久化）⇒ 只推进计时态。
- `ProcessRefillIndex`（`ScriptParsers.cs:838`）⑤-1 未暴露 ⇒ 置 0。
- 商人 `RM_TURN`/`RM_HIT`/`RM_DISAPPEAR`/`RM_SPACEMOVE_FIRE` 的**帧面**（`SM_*`）⇒ 接线层（⑤-4）。
- `NpcLimit` 墙钟预算 ⇒ 丢（`whitelist.md` T-42）。
- `MerchantState.castle_under_war` 缺省 `false`（攻城层 ⑧）。

## 8. 证据清单

`00-rust-rng-trace.tsv` / `01-oracle-anchors.txt` / `02-commands.txt` /
`03-merchant-rng-trace.txt` / `04-claimb-groupdiff.txt`（+ `04-{swap,set}-{a,b}.tsv`）/
`05-redchecks.txt` / `06-gates.txt` / `06b-workspace-test.txt` / `07-boundary.txt` /
`SHA256SUMS.txt` / `README.md`。

## 9. reviewer-5 `0f220cac` F-1/F-2 返工（本 head）

**F-1（Important）**：倒序挂图把两种彼此独立的 oracle 次序合并成同一个 Rust 槽位序 ——
`spawn_merchants` 原用 `spawns.into_iter().rev()` 一趟建实体 + 挂图 ⇒ EntityStore 槽位序 =
文件反序（`EntityStore::insert` 槽位序 = 插入序）⇒ `merchant_ids_sorted` / `merchant_tick`
的 pass 内 `Run()` 序与原 oracle（`MerchantList` 正序）相反。

- **改前**：`spawn_merchants` 单循环 `rev()`，实体槽位序 = `["B","A"]`。
- **改后**：拆 `World::insert_merchant`（建实体）/ `World::attach_merchant_to_map`（挂图）：
  正序 insert ⇒ 槽位序 = `["A","B"]`；倒序 attach ⇒ `obj_list[0]` = `"B"`（两个次序解耦）。
- **题**：用例 `spawn_merchants_decouples_tick_order_from_obj_list_order`（双指向）；
  `05-redchecks.txt` 的 F-1a/F-1b 两条。
- **RNG 流不变**：本次只改实体插入/挂图的**次序**，不增减取数（`00-rust-rng-trace.tsv`
  的 4 条站点值与次序逐字节不变，仅行号随代码位移 ⇒ 已重生）。

**F-2（Minor）文书自相矛盾**：已订正 `crates/world/src/merchant.rs` 模块头 / `merchant_ids_sorted` /
`merchant_tick` doc 与 `bins/game-svr/src/merchant.rs` 模块头（明说「实体槽位序 = `MerchantList`
序」「倒序只定 `obj_list` 追加序」），新增 oracle 锚点 `01-oracle-anchors.txt[11]` 的
（a）/（b）两个独立次序，并在 README §0/§1/§4/§5 同步。
