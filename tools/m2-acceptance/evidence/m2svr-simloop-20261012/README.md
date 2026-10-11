# M2 sim 循环对接片（`CM_PICKUP`/`CM_DROPITEM` 派发）证据

卡：`openmir2-m2-gamesvr-host`（owner §26 的「sim 循环对接」；接线层 `pickwire` 已在
前一片 `1b548d64` 交付，本片把它**接进 `sim_loop` 的命令派发**）。分支
`worktree-m2-gamesvr-host`。**本片未起栈、未占端口**（owner §26：窗口借给 `dbsrv6k`；
dbsrv6k 事后已写 `window-return` 交还）。

## 1. 交付内容（写集 = `mir2-rs/bins/game-svr/**`）

| 文件 | 改动 |
| --- | --- |
| `config.rs` | `[Setup]` 新增 `HumanMaxGold` / `FloorItemCanPickUpTime` / `ControlDropItem` / `InSafeDisableDrop` / `CanDropPrice` / `DropItemRage` 六个 `ground::*` 输入键；缺键用 oracle 源码缺省（`GameSvrConf.cs:1451/1488/1703/1704/1706/1754`），写了但非法即报错 |
| `strings.rs` | `[String] CanotPickUpItem`（oracle `MessageSettings.cs:221`，默认 `在一定时间以内无法捡起此物品!!!`） |
| `enter.rs` | 新增 `pub fn recalc_for_session(AbilityInputs, job) -> RecalcOutput`，把帧 52/752 的 `RecalcAbilitys` 求值抽出（**同一份实现**，会话态 `GroundState.wabil` 复用，不复制第二份） |
| `main.rs` | `sim_loop` 的 `GateEvent::Command` 派发：`CM_PICKUP(1001)`/`CM_DROPITEM(1000)` → `ground::{pick_up,drop_item}` → `pickwire::outcome_frames` → 按 `[GateIdx,Socket]` 回帧；进世界时从同一份 `CharacterData` 建立 `SessionGround`（`ItemList`/`Gold`/`WAbil` + `CanotPickUpItem`），断线移除 |

口径：`ground::pick_up`/`drop_item`（m3bag 交付，逐字段对 oracle 向量）+ `pickwire`
（前一片交付，逐条 oracle 行号）**不动**；本片只做「找会话 → world 只读视图装 `GroundMap`
→ 调纯函数 → 路由回连接」。`SM_GOLDCHANGED`/`SM_WEIGHTCHANGED` 用**调用后**的会话态
（oracle `GoldChanged()`/`WeightChanged()` 在 `Gold +=`/`AddItemToBag` 之后发）。

## 2. 判据（自动、无网络、无端口）

| 用例 | 判据 |
| --- | --- |
| `tests::pickup_gold_dispatches_and_updates_session`（bin） | 怪物击杀落金币 ⇒ `CM_PICKUP`：① 位置门不成立 ⇒ 不投递且金币不变；② 位置门成立 ⇒ 该角色网关线收到 `SM_GOLDCHANGED`，`Recog` = **改后** `Gold`，会话态金币同步增加；只回一帧 |
| `tests::bag_items_from_flattens_and_pads_desc`（bin） | `CharacterData.bag_items` ⇒ `GroundState.item_list`：展平空槽、`desc` 截断/补零到 14B |
| `config::tests::setup_drop_pickup_keys_and_defaults` | 六键解析 + 缺键 oracle 缺省 + 非法值报错（`DropItemRage=300` / `ControlDropItem=maybe`） |

### 红检（改坏 ⇒ 必红，原始输出见下）

| # | 改坏什么 | 结果 |
| --- | --- | --- |
| A | `pickwire::pickup_position_ok` 的 `&&` → `\|\|` | `pickup_gold_dispatches_and_updates_session` **FAILED**（`位置门不成立 ⇒ 不得投递`） |
| B | `pickwire::notice_frame` 的 `RM_GOLDCHANGED` 用 `0` 代替 `session.gold` | 同上用例 **FAILED**（`left: 0  right: 192`） |

## 3. 原始输出（证据四件套：命令 + 输出 + 路径 + hash）

- `green-pickup-tests.txt`：`cargo test -p game-svr --offline -- pickup_gold_dispatches_and_updates_session bag_items_from_flattens_and_pads_desc setup_drop_pickup_keys_and_defaults` ⇒ 3 passed / 0 failed。
- `redcheck-position-gate-mutated.txt`：红检 A（rc=101）。
- `redcheck-gold-poststate-mutated.txt`：红检 B（rc=101）。
- `redcheck-restored.txt`：还原后再跑同一命令 ⇒ 3 passed / 0 failed。

## 4. 门禁（隔离 target `E:/tmp/openmir2-m2svr-target`，合并树）

```
cargo fmt --all -- --check                                   # 0
cargo clippy --workspace --all-targets --offline -- -D warnings   # 0
cargo test --workspace --offline                             # 58 suites / 516 passed / 0 failed / 16 ignored（rc=0）
cargo run -p msgcodegen --offline -- --check                 # 604 OK
```

原始输出：`E:/tmp/openmir2-watch/pi-m2svr-simloop-{clippy,tests}.txt`。

## 5. 未做（登记，不静默）

1. **拾取删格对 `crates/world` 的写回**：owner §25.1 (A) 只批**只读分片**
   （`floor_items_view_sorted`）。本片调用 `ground::pick_up` 会改**临时** `GroundMap`，
   但 world 里的地面物不消失（客户端已由 `SM_ITEMHIDE` 隐藏）⇒ 重复拾取会再入包。
   **归下一片**，需 owner 按 §16 放行世界写分片。
2. `CM_DROPITEM` 成功写回（`Envir.AddItemToMap`）在本仓 oracle 下**恒失败**
   （`MapItem.ItemId == 0`，`ground.rs` 模块头 T-19）⇒ 恒回 `SM_DROPITEM_FAIL`（1:1）。
3. **`m2-06-pickup` 真机截图**：还缺「地面有物」的生产路径（世界只在下怪掉落时落地，
   客户端侧需先有可见地面物）⇒ 本片仍取不到（如实登记，不绕过）。
4. `Dealing` / `IsCanDrop` / `GroupOwner` / `DealLastTick` 等未建模状态按 false/0 走
   （无交易/组队/PK 状态系统），已在 `handle_ground_cmd` 注释逐条标注。

## 6. 上一片 review 的 F-1 / P-1 收口（option (b)：登记为已知缺失）

reviewer-3 `a3af6bf7`(I-1) / reviewer-5 `c541b6b4`(F-1) / reviewer-6 `90ae5a2b`+`21083c8d`(P-1)
三名审查者独立得同一结论：上一片 `smframe::sm_additem` 产 **header-only**，而 oracle `SendAddItem`
（`PlayObject.Base.cs:225-227`）在头部后追加 `SendSocket(ClientMsg, EDCode.EncodeBuffer(clientItem))`
**尾**（客户端/BotSrv 也按尾解析：`RobotPlayer.Operate.cs:838`）⇒ 该帧与 oracle 线缆体不等。

本片采 reviewer 给出的 **option (b)**（`ClientItem` 构造属物品展示层，未移植；见 T-24）：

1. `smframe.rs` 的 `sm_additem` 注释：删掉「**无尾**」的错误提法，改成「header-only，oracle 带头部 +
   `EncodeBuffer(clientItem)` 尾；已知缺失 T-24」；
2. `pickwire.rs` 的 `immediate_frames_map_to_oracle_bytes` 用例注释：明写「把当前 header-only 钉住，
   避免后续真机 pickup 把缺尾误当 parity」（断言不变，仍钉 `series=1` + header-only）；
3. 证据 README（本片 + `m2svr-pickwire-20261012/`）同步改正措辞；
4. **白名单登记 T-24**（`mir2-rs/tests/parity/whitelist.md`）：锚 `PlayObject.Base.cs:225-227`，
   写清「未实现、线上不可达、承接 = 物品展示层独立片、不得拿 header-only 当 parity」。

> 这三条 finding 针对的是上一片 head `04523f5b`；本片 head 已含上述收口。上一片 `patch cf7e96f1`
> 的 `request_changes` 结论由本片（新 head）覆盖。
