# CM_PICKUP / CM_DROPITEM 接线层（`pickwire`）—— 映射与装配证据

- 卡：`openmir2-m2-gamesvr-host`（M2 验收② 的 `pickup` 格）+ `openmir2-m2-enter-world-frames`
- 口径来源：作者 comment `d552d2a2`（输入清单）；owner §25.1（world 只读分片）+ §26（写接线代码，窗口借出期间不抢端口）
- 本片**未起栈、未占端口**（仅 `cargo test` / `cargo clippy`）

## 1. 交付

- `mir2-rs/bins/game-svr/src/smframe.rs`：新增 `CM_PICKUP`/`CM_DROPITEM` 的 `SM_*` 编码器
  （`sm_additem` / `sm_itemhide` / `sm_weightchanged` / `sm_goldchanged` / `sm_menu_ok` / `sm_dropitem_reply`），
  逐条照 oracle 落位（`Recog/Param/Tag/Series` + 尾 `EncodeString(Msg)`）。
- `mir2-rs/bins/game-svr/src/pickwire.rs`（新）：接线层
  - `immediate_frame`：`Frame` → `SM_*`（`SM_ADDITEM` 现为 **header-only，尾未接 ⇒ 已知缺失 T-24**，oracle `SendAddItem` 带 `EncodeBuffer(clientItem)` 尾；`SM_DROPITEM_SUCCESS/FAIL` 补 `EncodeString(请求 Msg)`）；
  - `notice_frame`：五条 `RM_*` → `SM_*`（`RM_WEIGHTCHANGED`/`SM_GOLDCHANGED` 用**当下**会话态）；
  - `outcome_frames`：立即帧 + 延时通知的完整下行序；
  - `build_ground_map`：world 只读视图（`floor_items_view_sorted`）→ `ground::GroundMap` 装配；
  - `pickup_position_ok`：`CM_PICKUP` 的派发层位置门。

## 2. 单测锚点

`cargo test -p game-svr --offline --lib pickwire` ⇒ 5/5 ok：

| 用例 | 锚点 |
| --- | --- |
| `immediate_frames_map_to_oracle_bytes` | `SM_ADDITEM`=200 series=1 **header-only（已知缺失 T-24：oracle 带 `EncodeBuffer(clientItem)` 尾，本用例把这现状钉住）**；`SM_DROPITEM_FAIL`=601 / `SUCCESS`=600，recog=make_index，尾=`EncodeString(Msg)` |
| `notices_map_to_oracle_frames` | `SM_ITEMHIDE`=611（recog=item_id,param=x,tag=y）/ `SM_WEIGHTCHANGED`=622（校验和 `((W+A+H)^0x3A5F)^0x1F35^0xaa21`）/ `SM_GOLDCHANGED`=653 / `SM_SYSMESSAGE`=100（series=1）/ `SM_MENU_OK`=767 |
| `sysmessage_without_text_is_skipped` | 缺 `MessageSettings.CanotPickUpItem` 文本 ⇒ 不产出（缺口 1，不编造空文本） |
| `build_ground_map_matches_view` | 只装目标图；金币 `item_id`/`count`/归属、物品模板名逐项对上 world 视图 |
| `pickup_position_gate` | `CurrX==nParam2 && CurrY==nParam3` |

## 3. 红检（改坏 ⇒ 必红；原始输出见本目录）

| 红检 | 改回什么 ⇒ 必红 | 原始输出 |
| --- | --- | --- |
| ① `SM_ADDITEM` 位序 | `sm_additem` 的 `series` 1→0 ⇒ `immediate_frames_map_to_oracle_bytes` FAILED（`left …0,0` vs `right …1,0`） | `redcheck-sm-additem-series-mutated.txt` / `...-restored.txt` |
| ② 地面装配 `item_id` | `build_ground_map` 的 `item_id: v.item_id` → `v.item_id + 1` ⇒ `build_ground_map_matches_view` FAILED（`left 70002 right 70001`） | `redcheck-ground-item-id-mutated.txt` / `...-restored.txt` |

## 4. 已知缺口（登记，不静默；属下一片）

1. **sim 循环对接未做**：本片只交付接线层；`main.rs` 的 `GateEvent::Command` 派发、per-session
   `GroundState`（背包/金币/`WAbil`，进世界解析出 `CharacterData` 后现即丢弃）、
   以及拾取删格对 world 的**写回 API**（会改世界状态，需 owner 按 §16 放行写分片）为下一片。
2. **`RM_SYSMESSAGE` 文本**：oracle `MessageSettings.CanotPickUpItem` 由配置提供，`ground::pick_up`
   的 `QueuedNotice` 不带文本 ⇒ `pickwire` 用 `WireSession.sys_msg` 显式注入，缺则不产出。
3. **`BagItem.dura/dura_max/desc`**：不在 world 只读视图（`FloorItemMeta` 只记 `kind/index/count/item_id/
   of_base_object`）⇒ 装配取默认值；逐字节复原需扩 world 记账。
4. **`CM_DROPITEM` 成功写回**：本接缝下恒失败（`MapItem.ItemId == 0`，`ground.rs` 模块头 `T-19`）⇒ 不加地面。
5. 因此 **`m2-06-pickup` 截图的真机前置仍未满足**（需第 1 条的对接完成）。

## 5. 残留物清单

- 本目录：README + 4 个红检原始输出（入库）。
- 无仓库外残留物；未起栈、未占端口、未改现网配置。
