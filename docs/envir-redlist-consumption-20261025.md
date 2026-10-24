# Envir redlist 生产消费点复核（L 片）— 2026-10-25

- 卡：`openmir2-envir-redlist-batch`（L 片；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/envir-redlist-consumption`）。
- 基线：本地 `master` `e510e78e1`；T3 四牙已在 `0bac2ed2d`（直接复用，不重做）。
- 写集：本文件 + `mir2-rs/bins/game-svr/src/ctor_fold.rs`（仅**新增一条合同测试**）。
- 红线：未改解析层（`lists.rs` / `noclearmonlist.rs` 零 diff）、未改 `src/**`（C#）、未起栈/占端口/连库。

## 0. 一句话结论

八类名单逐点核完：**三件已接线且等价**（ItemNameList / AdminList / UnbindList）、**Robot 的 RNG 账本已接线**
（新增两份解析器的等价合同测试）、**四件在 oracle 侧本来就是 no-op 或消费点被注释**
（DisableSendMsg / DenyChrName Envir / GameLogItemName / NoClearMonList）⇒ **没有发现必须接线的生产缺口**。
唯一发现的 action 级缺口（CLEARMAPMON 本体在 Rust host 是 no-op）**不属名单消费**，单独登记（见 §3）。

## 1. 逐点清单（文件:行号 + 用途 + 判定）

| # | 名单 | Rust 生产装载 | Rust 生产消费点 | oracle 对照 | 判定 |
|---:|---|---|---|---|---|
| 1 | `ItemNameList.txt` | `bins/game-svr/src/lib.rs:622-640`（`load_item_name_list_file` → `set_item_names` `:633`） | `bins/game-svr/src/ability.rs:339-346`（`StdItemCatalog::custom_item_name` → `mir2_data::lists::get_custom_item_name`）⇒ 背包/商人自定义物品名 | `GameApp.cs:166` `LoadCustomItemName`（`CustomItemSystem.cs:80-112`）；`GetItemName` `:119-125` → `Merchant.cs:1785-1797` | **已接线（等价）** |
| 2 | `AdminList.txt` | `lib.rs:885-908`（`load_admin_list_file`），存入 `enter_cfg`（`:1200`、`:18015`） | 查表半 `bins/game-svr/src/adminlist.rs:114-115`（`get_hum_permission` → `lookup_admin`）——**故意不接**到权限路径 | 装载 `GameApp.cs:209`（`LocalDB.cs:16-97`）；唯一读者 `GetHumPermission`（`WorldServer.cs:1021-1039`）的调用点 `PlayObject.cs:1188-1194` **被注释** ⇒ 权限恒 0 | **装载已接线 + 读取按 oracle dead（等价）** |
| 3 | `UnbindList.txt` | `lib.rs:916-938`（`load_unbind_list_file`），存入 `:1201` / `:18019` | `lib.rs:8085-8219`（`CM_EAT` StdMode 31 / AniCount==0 解包按 `StdItem.Shape` 查名） | `GameApp.cs:173-179`（`LocalDB.cs:872-915`）；消费 `PlayObject.Operate.cs:657-666` | **已接线（等价）** |
| 4 | `Robot.txt` | 生产走 `crates/envir/src/robot.rs:65 parse_robot_lines`，经 `bins/game-svr/src/ctor_fold.rs:71-85`（`MapLoader::load_robot`） | 构造段 RNG 账本（每有效行 **4 抽**；`push_robot_objects`）；**世界实体/AutoBot 消费未移植**（`ctor_fold.rs:45` 已登记） | `GameApp.cs:58` `new RobotManage()` → `RobotManage.cs:21-54`；实体由 `AutoBot` 脚本宿主消费（未移植） | **已接线（RNG 账本）；实体层缺口登记** |
| 5 | `NoClearMonList.txt` | **无装载**（oracle `GameApp.cs:109` 会装载进 `M2Share.NoClearMonLIst`） | **无消费**；`ActionOfClearMapMon` 的 Rust host hook 是默认 no-op（`crates/script/src/engine/mod.rs:687-688`），action 本体未接 | 装载 `M2Share.cs:1746-1766`；**getter 整段注释**（`M2Share.cs:1782-1794`）⇒ 列表从不被读；消费点 `ExecutionProcessingSys.cs:2994` 恒 false | **列表等价 no-op；action 缺口另登记（§3）** |
| 6 | `DisableSendMsgList.txt` | 无装载（`load_disable_send_msg_list_file` 恒 false，主动路径不调用） | 无消费；`chat.rs` 的 `FilterSendMsg` 支未建模（既有登记） | oracle loader `M2Share.cs:1355-1374` 与 getter `:1498-1510` **整段注释** | **等价 no-op** |
| 7 | `DenyChrNameList.txt`（Envir） | 无装载（`load_deny_chr_name_list_file` 恒 false） | 无消费 | oracle loader `M2Share.cs:1626-1652` 与 getter `:1654-1666` **整段注释** | **等价 no-op** |
| 8 | `GameLogItemNameList.txt` | 无装载 | 无消费（`gamegold.rs:31` 已登记「不建模日志字段」） | oracle 装载 `M2Share.cs:1512-1536`；消费判定 `MySqlDB.cs:87-90`，但**所有 `AddEventLog` 调用都被注释**（`BaseObject.cs:761`、`UserCastle.cs:801/830/866`、`PlayObject.Base.cs:83/876`、`PlayObject.Message.cs:458/477/499`） | **等价 no-op（无可观测行为）** |

> **易混点（已在表内区分）**：`DenyChrName.txt`（DBSrv 文件，`src/DBSrv/AppService.cs:31`、`DBShare.cs:36`）
> **不是** Envir 的 `DenyChrNameList.txt`。Rust 的 DB 侧 `bins/db-srv/src/route.rs:61` + `handler.rs:426`
> 已等价实现前者；Envir 侧名单按 oracle 保持 no-op。

## 2. 本片唯一的代码改动：Robot 两份解析器的等价合同测试

- 生产链用 `mir2_envir::robot::parse_robot_lines`（`crates/envir/src/robot.rs:65`）；T3 片交付的
  `mir2_data::lists::parse_robot_list`（`crates/data/src/lists.rs:389`）**没有生产消费者**——这是一份
  重复实现。为把「已接线 = 等价」变成机判，新增
  `bins/game-svr/src/ctor_fold.rs::tests::robot_txt_parsers_agree_on_deployed_and_edge_fixtures`：
  三组夹具（部署件形态 / 注释+空行+单列+三种分隔符 / CRLF+LF 混排）逐行比对两份解析器。
- 牙（实测）：把 `data` 侧 `MONSTER_SPIT_CONST` 的 `'/'` 去掉 ⇒ 本测试 **FAILED**；还原 ⇒ **ok**。
- 结论：两份解析器**逐行等价**（行过滤、`{ ' ', '/', '\t' }` 分词、两列非空入库门、CRLF/LF 吸收均一致）；
  重复实现保留（不合并，避免无收益的跨 crate 迁移），以本测试钉住分叉。

## 3. 发现的非名单缺口（只登记，不在本片修）

1. **`ActionOfClearMapMon`（CLEARMAPMON 215）的 host hook 是 no-op**：oracle 会清地图怪物
   （`mon.NoItem = true; mon.MakeGhost()`，`ExecutionProcessingSys.cs:2983-3001`）；Rust
   `crates/script/src/engine/mod.rs:687-688` 的 `clear_map_monsters` 默认空实现，`bins/game-svr` 未覆写
   ⇒ 该脚本动作在生产是 no-op。属**世界实体状态变更**（非名单消费），建议另开卡按 oracle 实现
   （NoClearMonList 的 getter 仍照搬 false）。
2. **`cards.json` wave4 W3 分组缺失**（owner 事）：`mir2-rs/tests/parity/coverage/cards.json:91` 已有
   `content:NoClearMonList.txt` 条目（含 oracle 恒 false + T-60 证据），但 wave4 **W3 分组本身不存在**；
   本片只登记，不处理（原卡 `C-1` 口径：指派载体属 owner/coordinator）。

## 4. 门禁

- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace` / `cargo run -p msgcodegen -- --check` 全 rc=0。
- 7 项卫生门禁（conflict-marker / stale-pointer / dynamic-count / board-coverage / t_reservation /
  whitelist-anchor / m3-registry）全 rc=0；`git diff --check` rc=0；`git merge-tree` 无冲突（见 patch body）。
- `crates/data/src/lists.rs` / `crates/envir/src/noclearmonlist.rs` **零 diff**；C# `src/**` 零改动。

## 5. 残余

- `RobotObject` 世界实体/AutoBot 驱动器仍未移植（`ctor_fold.rs:45` 原登记，不属本片）。
- CLEARMAPMON 的 host 实现（§3.1）与 cards.json W3 分组（§3.2）只登记。
- 无临时物；本文件 + 一条测试是全部仓库改动。
