# M1c-2b 实施规格：ground / bag / querybag / storage 区 k 接线（2026-11-23，只读规格）

> 父 `1b23fc99c`（= 2a 规格对码版）。**只定实施规格、不写码**。前置：2a（make_index 统一单计数器）
> 已合并；**本片不碰 `item_number`**（见 §6 边界）。引用以**符号锚**为准（`lib.rs` 仍在被 2c/其他片改动）。

## 0. 范围与一句话

- **范围内 7 个 ident**：`CM_PICKUP(1001)` / `CM_DROPITEM(1000)`（ground）；`CM_TAKEONITEM(1003)` /
  `CM_TAKEOFFITEM(1004)`（bag）；`CM_QUERYBAGITEMS(81)`（querybag）；`CM_USERSTORAGEITEM(1031)` /
  `CM_USERTAKEBACKSTORAGEITEM(1032)`（storage）。
- **一句话**：按 B 组同一模式——**区 0 的 `INBOUND_PROCESS` 臂一行未动**，只在
  `handle_zone_command_on` 加**同 handler** 的区 k 臂；`zone_cmd_class` 把 7 个 ident 归 `Local`
  （仅供计数，不参与处置）；`zone_cmd_unsupported` 不再计这 7 个。
- **不在范围**：shop 其余（`CM_CLICKNPC`/`CM_MERCHANT*`）、`CM_EAT`（2c）、`CM_DEAL*`、`CM_SELLOFF*`
  ——区 k 保持 `Unsupported` 并计数（守恒牙见 §4）。

## 1. 每族的派发点与实参来源

统一模式（照 B1/B2/B3）：`handle_zone_command_on` 现有签名
`(world, displays, online, ground_sessions, saves, gates, chr_name, socket, cmd, tail, env: &ZoneCmdEnv)`
；**本片只需新增一个形参** `trade_sessions: &HashMap<String, game_svr::trade::DealState>`
（ground/bag 用；调用方 `handle_zone_command` 包装与 `tick_phase_a` 的执行闭包同步补，取
`host.trade_sessions`，同一把 `state` 锁内）。

| 族 | 区 0 handler（符号锚） | 区 k 臂的实参来源 | 分类改动 |
| --- | --- | --- | --- |
| ground | `handle_ground_cmd` | `cmd.*`；`trade_sessions`（新形参）；`&*world`（**只读**）；`ground_sessions`；`online`；`gates`；`&env.enter_cfg.ground`；`env.colors`；`&env.enter_cfg.catalog`；`env.registry`；`&env.enter_cfg.string_conf` | `Local` |
| bag | `handle_bag_cmd_with_magic` | 同区 0 调用点：`online`、`ground_sessions`、`gates`、`&env.enter_cfg.catalog`、`&mut world`、`&mut displays`、`env.enter_cfg.ref_msg_range`、`dealing = trade_sessions.get(chr).is_some_and(|s| s.dealing)`、`mir2_client_ability_layout()`、`magic = saves.get_mut(&chr).map(|saved| MagicRecalc { saved, cfg: magic_cfg_of(&env.enter_cfg), flame_ring_in })`（`flame_ring_in` 由 `online`→`world.entities` 读，区 k 本地） | `Local` |
| querybag | `handle_query_bag_cmd` | `chr`、`socket`、`online`、`ground_sessions`、`gates`、`&env.enter_cfg.catalog`、`&*world` | `Local` |
| storage | `handle_storage_cmd` | `cmd.*`、`online`、`ground_sessions`、`gates`、`&mut world`、`&env.enter_cfg.catalog`、`&env.enter_cfg.shop`、`env.colors` | `Local` |

**ident 判据（写死）**：
- ground：`game_svr::ground::CM_PICKUP` / `CM_DROPITEM`；
- bag：`game_svr::bag::CM_TAKEONITEM` / `CM_TAKEOFFITEM`；
- querybag：`mir2_protocol::messages::CM_QUERYBAGITEMS`；
- storage：`game_svr::shop::CM_USERSTORAGEITEM` / `CM_USERTAKEBACKSTORAGEITEM`（**只这两条**；区 0 的 `INBOUND_PROCESS` 支也只路由这两条）。
- 分类器改动：`zone_cmd_class` 的 `ident_level` 里加这 7 条（或加专用谓词，但**必须单源**）；
  同步删掉函数体注释里"表外一律 `Unsupported`（含 `CM_QUERYBAGITEMS(81)`…）"的举例。
- **测试连带**：`zone_cmd_class_m1b_families_are_local` 里 `zone_cmd_class(CM_QUERYBAGITEMS, None) ==
  Unsupported` 的断言要改成 `Local`，并把 7 个 ident 全部加入该测试的 Local 清单。

## 2. A9 类检查（逐族，带证明锚）

**方法**：对四个 handler 函数体做**标识符扫描**（`guilds|castle|npc_host|npc_vars|merchant_book|
sell_off_book|party_cfg|gm_*|item_number(_ex)|db|engine|saves|online_hum_gate|ids_online` 等），再对
**一层 callee** 逐个核签名（防"handler 干净、callee 借进程态"）。

**扫描结果（`1b23fc99c` 实测）**：
- `handle_ground_cmd`：体 0 命中。callee：`game_svr::ground::{pick_up, drop_item}`（`PickUpContext`/
  `DropContext` + `&mut GroundMap` + `&mut GroundState`，全纯）；`pickwire::{outcome_frames,
  build_ground_map}`（`Frame/QueuedNotice/WireSession/FloorItemView` + catalog）；`ability_tail`/
  `useitem::SysMsgTexts::from_conf`（配置）。**无进程态**。
- `handle_bag_cmd_with_magic`：体 0 命中（唯一的 `gm_*` 命中是 `smframe::gm_data_body` 的函数名，非进程态）。
  callee：`bag::{take_on, take_off}`（`EquipState`/`TakeOnContext`/world）；`smframe::*`；`itemwire::*`；
  `trade::add_item_tail`；`downlink::my_light`；`broadcast_ref_msg_with_self*`/`broadcast_feature_changed*`
  （online/gates/world/displays）；`apply_ability_to_entity`（`EntityMut`+`Ability`）。**无进程态**；
  `MagicRecalc` 的 `saved` 来自**区 k 自己的 `HostLoop.saves`**（落地区存档表）。
- `handle_query_bag_cmd`：体 0 命中。callee：`itemwire::{sm_bagitems, bagitems_tail}`、
  `ItemCatalog::{get, item_name}`。**无进程态**。
- `handle_storage_cmd`：体 0 命中。callee：`shop::{storage_item, take_back_storage_item}`（`GroundState`/
  `Vec<BagItem>`/`ShopContext`）；`shop_context`（catalog/shop_cfg/colors）；`emit_shop_frames`（gates）。
  **无进程态**。

**结论：四族零进程态，可直接搬。没有 A9 意外命中。** 两个"看起来像状态面"的点不在 A9 范畴：
1. `handle_ground_cmd` 的 `world: &World` 是**只读分片**（`floor_items_view_sorted`），地面写回**本来
   就不在模型内**（T-25：oracle 地面物不可达、回写会制造偏离）——不是进程态，也**不要**在 2b 顺带修；
2. `bag` 的 `magic` 组装读 `saves`（区 k 本地）与 `enter_cfg`（区副本）。

**若将来意外命中**（某人给 handler 加了进程态）：处置 = **该族退回 `Unsupported` 计数 + 开差异卡**，
**禁止**把 `ProcessServices` 借给区 k（A9 红线）。

## 3. 可观测副作用与牙（每族一条"直接 return 就红"）

> 牙形照 B 组：单测直调 `handle_zone_command_on`（或经 seam），断言 **`ZoneCmdOutcome::Dispatched`**
> + **一个可观测副作用**（gates 收到的帧 ident/字段、或会话状态迁移）。**只断言 `Dispatched` 不够**——
> "臂里直接 `return Dispatched`"必须红。

| 牙 | 构造 | 断言 | 变异（必红） |
| --- | --- | --- | --- |
| `zonek_ground_drop_is_oracle_fail` | 背包有可丢件；投 `CM_DROPITEM` | gates 收到 **601 `SM_DROPITEM_FAIL`**（T-19：oracle `MapItem.ItemId` 恒 0 ⇒ 正常丢弃**恒失败**、背包不动）；`sg.state.item_list` 不变 | 臂内直接 `return Dispatched` ⇒ 无 601 ⇒ 红。**不得**断言"地面出现/600"（不可达面，见 T-19/T-21） |
| `zonek_ground_pickup_adds_item` | **直插**世界地面物（`ItemId != 0`，与 `ground-probe` 夹具同法）；投 `CM_PICKUP` | gates 收到 **200 `SM_ADDITEM`**（尾=ClientItem）；`item_list` 增 / `gold` 增（金币支为 653） | 直接 return ⇒ 无 200 ⇒ 红 |
| `zonek_ground_pickup_empty_noop` | 地面无物（生产可达形态，T-25） | 零帧、状态不变、不 panic（**守恒**：别把"不可达"当失败） | 改坏位置门/加帧 ⇒ 红 |
| `zonek_bag_takeon_ok` / `..._takeoff_ok` | 夹具背包可穿戴件/已穿件 | **615 `SM_TAKEON_OK`** / **619 `SM_TAKEOFF_OK`**；`UseItems`/`ItemList` 迁移（与 bag-probe 同口径） | 直接 return ⇒ 无 615/619 ⇒ 红 |
| `zonek_bag_reject_frames` | 不可穿戴/空槽 | **616/620**（失败 ident） | 同上 |
| `zonek_querybag_bagitems` | 背包有件 | **201 `SM_BAGITEMS`**，`Recog=ActorId`、`Series=ItemList.Count` + 尾；二次调用在 30s 内 ⇒ 无帧（仅 WRN，oracle 同） | 直接 return ⇒ 无 201 ⇒ 红 |
| `zonek_storage_ok` | 夹具商人 `IsStorage` + 同图 ≤15 格 | **`SM_STORAGE_OK`** / **`SM_TAKEBACKSTORAGEITEM_OK`**；`storage_items`/`item_list` 迁移 | 直接 return ⇒ 无帧 ⇒ 红 |
| `zonek_storage_reject` | 商人无 `IsStorage` / 仓满 | **`SM_STORAGE_FAIL`** / **`SM_STORAGE_FULL`** 等 | 同上 |

所有牙共同要求：①行为量（执行/未执行、帧 ident/关键字段、状态迁移）；②计数 delta（`drained` +1；
`zone_cmd_unsupported` 增量 0——用 `ZoneCmdOutcome` 判，避免并行用例互踩）；③变异证据与恢复 sha 相等。

## 4. 零行为证据

1. **区 0 一行未动**：本片**保留**区 0 `INBOUND_PROCESS` 支的全部臂（`CM_PICKUP/DROPITEM`、
   `CM_TAKEON/OFF`、`CM_QUERYBAGITEMS`、仓库两条），只新增区 k 臂 ⇒ 区 0 行为按构造不变。
   报告必须签名"**区 0 的 `INBOUND_PROCESS` 臂一行未动**"。
2. **N=1 两树**：master-vs-branch、归一化 stdout 逐字节（口径同 M1c-2 §4.3）；若出现差异 ⇒ 按缺陷处理。
3. **每族一牙 + 变异**（§3 表）；**未接线族守恒牙**：同一单测里枚举 `CM_CLICKNPC`、`CM_MERCHANTDLGSELECT`、
   `CM_EAT`、`CM_DEALTRY`、`CM_SELLOFFADDITEM` ⇒ `ZoneCmdOutcome::Unsupported` 且
   `zone_cmd_unsupported` 增量 >0（防本片顺带接线）。
4. **分类器测试**：`zone_cmd_class_m1b_families_are_local` 增 7 个 Local ident；
   `CM_QUERYBAGITEMS` 的 Unsupported 断言翻转；其余分类器牙（payload 例外、唯一性、接线只在一处）不变。
5. **既有探针/金样**：`ground-probe`/`bag-probe`/`querybag` 单测全绿（本片不改 handler 本体）。

## 5. 四族内部顺序建议

1. **querybag**（最小：纯读、201 帧最好观测、无新形参）——先落"区 k 派发骨 + 分类 + 守恒牙"。
2. **bag**（夹具成熟：bag-probe 21 用例；需组装 `trade_sessions`/`MagicRecalc`；帧 615/616/619/620 可观测）。
3. **ground**（有 T-19/T-25 两个"恒失败/不可达"登记，牙要避不可达面；需 `trade_sessions`）。
4. **storage**（需商人实体夹具 + 两列表迁移；帧最多但夹具最重）。

理由：观测面成熟度/夹具成本/状态面由小到大；每族一提交，可单独回退。

## 6. 与 2a / 2c 的边界（写死）

- **不碰 `item_number`**：不改 `ItemNumberCounter`/`SharedServices.item_number`/GM-M/`production_world`/
  `new_zone_state`/`zone_state` 任何计数相关代码（全是 2a 的事）。
- **不碰 `CM_EAT`**：`CM_EAT` 仍归 `Unsupported`（2c 用分支门 + 类型化邮箱接管）；
  不新增/不修改 `ControlRequest`。
- **不碰 `world.rs` 的 `staging_dirty` 段**（A22，`a7` 证据面）。
- 不改 `smframe.rs`/帧编码；不改 `zone_cmd_class` 的 payload 三态逻辑（只说 ident 级新增）。
- **与 2a 并行时**：本片唯一签名改动 = `handle_zone_command_on`(+`trade_sessions`) 与其调用方
  （`handle_zone_command` 包装、`tick_phase_a` 执行闭包）；与 2a 的构造点签名无交集，rebase 无冲突。

## 7. 交付前自查

- [ ] 7 个 ident 全部分类 `Local` + 派发臂；分类器注释与 `CM_QUERYBAGITEMS` 断言已改；
- [ ] 每族牙（§3 表）+ 变异（"直接 return"必红）+ 恢复 sha 相等；
- [ ] 未接线族守恒牙（`CLICKNPC/MERCHANT*/EAT/DEAL*/SELLOFF*` ⇒ Unsupported 且计数增）；
- [ ] 区 0 `INBOUND_PROCESS` 臂一行未动（报告签名）；N=1 两树 stdout 逐字节；
- [ ] 无调试打印残留（`eprintln`/`dbg!` 清零或 env 门控+提交写明）；
- [ ] 门禁：`cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace` / `msgcodegen --check` /
      `docs_anchor_gate --prose`（符号锚）。
