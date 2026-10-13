# M3⑤-实现 5 第①片 —— 仓库会话态与存档落库（`openmir2-m3-storage-persist`）

> 卡：`openmir2-m3-storage-persist`（issue `e921d442`，`owner_hint = openmir2-worker-11`）。
> 分支 `storage-persist-host`，base = `refs/heads/master`（`15510a47`），代码 HEAD `0b731d1d`。
> 本片**只**做 oracle 的「仓库会话态（`StorageItemList` + 39 上限 + `IsCanGetBackItem` 取回锁）+
> 存档落库」；两条 `CM_*` 收包派发与帧面（`CM_USERSTORAGEITEM(1031)`/
> `CM_USERTAKEBACKSTORAGEITEM(1032)` + `SM_STORAGE_*`/`SM_SAVEITEMLIST`）**归 ⑤-4 第②片**
> （见 `bins/game-svr/src/shop.rs` 模块头的「本片未做（登记，第②片）」段，以及 `whitelist.md` T-47）。

## 1. 交付内容

### 1.1 新模块 `bins/game-svr/src/storage.rs`

| 本模块 | oracle 锚点 |
| --- | --- |
| `StorageState { items, is_can_get_back_item }` / `new()` | `PlayObject.cs:207/741/878/956` |
| `StorageState::can_store()`（`len < 39`） | `PlayObject.Operate.cs:1629` |
| `bag_item_from_server`（`ToClientItem` 子集） | `ServerUserItem.cs:39-50` |
| `storage_items_from_record`（读档） | `WorldServer.cs:1213-1227` |
| `load_from_record`（读档 + 初生锁开） | `WorldServer.cs:1213-1227` + `PlayObject.cs:956` |
| `storage_items_for_save`（存档 50 槽） | `WorldServer.cs:1359-1376` + `PlayerDataInfo.cs:117` |

**存档长度口径的关键前提**：`MakeSaveRcd` 的载体是**新建**记录（`WorldServer.cs:1233-1234`
`CharacterDataInfo humanRcd = new CharacterDataInfo(); CharacterData playerInfo = humanRcd.Data;`，
`Data.StorageItems` 恒是构造期 `new ServerUserItem[50]`）⇒ 存档数组**恒 50 槽**、每次从空槽起、
不会被上一次的旧物品顶掉。Rust `storage_items_for_save` 照此实现（不取旧记录长度/内容）。

### 1.2 宿主接线（`bins/game-svr/src/main.rs`）

- `SessionGround` 增 `storage: storage::StorageState`（10 处构造点同步）。
- 进世界：`storage::load_from_record(d.storage_items.as_deref())`（`CharacterData.StorageItems` → 会话）。
- 存档：`build_save_record` 写 `rec.data.storage_items = Some(storage::storage_items_for_save(&sg.storage.items))`
  ⇒ 生产编码 → DBSvr → `characters_storageitem`（`PlayDataStorage.Save.cs:62/360`）。

## 2. 判据与牙齿

- **新用例 10 条全绿**（见 `03-newtests.txt`）：`storage` 模块 8 + 宿主接线 2
  （`save_record_persists_storage_items_as_fifty_slots`、`enter_loads_storage_items_from_record`）。
  宿主用例经**生产编码** `record::encode_save_character_data` + **DBSvr 侧类型**
  `mir2_storage::SaveCharacterData::from_bytes` 解回断言（真身往返，不是 Rust 内部自洽）。
- **改坏必红 4 条**（见 `04-redchecks.txt`，逐条 mutation ⇒ FAILED、还原 ⇒ ok）：

  | 编号 | mutation | 判红用例 |
  | --- | --- | --- |
  | RC1 | `storage_items_for_save` 尾槽填充删掉（`Some(default)`→`None`） | `save_is_fixed_fifty_slots_with_default_filler` / `save_record_persists_storage_items_as_fifty_slots` |
  | RC2 | `storage_items_from_record` 的 `Index > 0` 门删掉（`if true`） | `load_skips_null_and_zero_index_keeps_order` / `enter_loads_storage_items_from_record` |
  | RC3 | `build_save_record` 不写 `rec.data.storage_items` | `save_record_persists_storage_items_as_fifty_slots` |
  | RC4 | `can_store` 边界 `<`→`<=` | `can_store_boundary_is_thirty_nine` |

## 3. 门禁（`05-gates.txt` / `05b-workspace-test.txt`）

- `cargo fmt --all -- --check` rc=0
- `cargo clippy --workspace --all-targets --offline -- -D warnings` rc=0
- `cargo test --workspace --no-fail-fast --offline` **94 suites · 888 passed · 0 failed · 21 ignored**（rc=0）
- `cargo run -p msgcodegen --offline -- --check` rc=0（604 条常量）

## 4. 边界（`06-boundary.txt`）

`git diff --name-only 15510a47..HEAD -- src sql MirClient` = **空**（oracle/C#/客户端/表结构零改动）。

## 5. 未做（登记，不静默跳过）

- 两条仓库 `CM_*` 派发 + 帧（`SM_STORAGE_OK/FULL/FAIL`、`SM_TAKEBACKSTORAGEITEM_*`、
  `SM_SAVEITEMLIST` 分页）⇒ **⑤-4 第②片**。本片只钉住会话态、39 上限、锁与存档往返。
- `IsCanGetBackItem` 的密码锁降级（`PlayObject.cs:1451-1454`，`@SetPassword`/`@Lock`/`@Unlock`）：
  本宿主未装载密码锁脚本/配置 ⇒ 会话恒初生 true（= oracle 未加锁态），锁为 false 的分支不可达。
- 真机两侧同序列对拍（`run_npc_shop_pair.ps1`）等 ⑤-4/⑤-5/⑤-6 落地 + 独占窗口；本片**未起栈、未占端口**。

## 6. 证据文件

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 行号锚点（含实测 grep/片段） |
| `02-commands.txt` | 复现命令清单 |
| `03-newtests.txt` | 10 条新用例原始输出 |
| `04-redchecks.txt` | 4 条改坏必红（mutated/restored） |
| `05-gates.txt` / `05b-workspace-test.txt` | 四门禁 + workspace 测试全文 |
| `06-boundary.txt` | 边界（`src/sql/MirClient` 空） |
| `SHA256SUMS.txt` | 仓库 LF/git-blob 口径（`--check-extra` + `eolcheck` 全绿） |

## 7. 残留物

- `E:/tmp/m2svr-target`（隔离 `CARGO_TARGET_DIR`，复用）。
- `E:/tmp/m2svr-gates-storage/**`（门禁/红检原始件，已拷入本目录）、
  `E:/tmp/m2svr-redcheck-storage{,2}.sh`（红检驱动，未入库）。
- worktree 干净；共享主检出未写/未编译；十端口开工前后一致（仅共享 3306）。
