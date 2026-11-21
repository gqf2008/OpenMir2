# `SpaceMove` 后 `VisibleActors` 未清空/重建（Rust ↔ oracle 行为差异）

> 发现于 `card/magic-push-batch-impl`（MG-35 瞬息移动接线）。
> **本卡只登记，不在该批修**：帧面不可见、爆炸半径超出 magic、缺独立观测面与牙。

## 1. oracle 行为

`src/M2Server/Actor/BaseObject.cs` 的 `SpaceMove`（`1103` 起），`FindMap` 成功后无条件执行：

1. `Envir.DeleteFromMap(CurrX, CurrY, CellType, this.ActorId, this)`；
2. **`VisibleHumanList.Clear()`**，并把 `VisibleActors` 逐项置 `null` 后 **`Clear()`**；
3. `Envir = envir`、落点 `CurrX/CurrY` 改写；
4. `SendMsg(RM_CLEAROBJECTS)` / `SendMsg(RM_CHANGEMAP)` /（`nInt == 1` 时）`SendRefMsg(RM_SPACEMOVE_SHOW2, …)`。

**同图瞬移也走这条路**（`MapRandomMove` 无条件调 `SpaceMove`）⇒ 目标图 == 当前图时**同样清空视野**、同样发完整换图簇。

## 2. Rust 现状

- 生产路径上 `visible_actors` 的写点：本轮之前**没有任何**"清空/重建"点；
- 唯一的 `clear()` 在 `mir2-rs/crates/world-bevy/src/aoi.rs` 的 AOI 重建内部（与 `SpaceMove` 无关）；
- `cast_space_move_spell_with`（`mir2-rs/crates/world-bevy/src/world.rs`）与换图簇（`mir2-rs/bins/game-svr/src/sessionwire.rs`）
  都**没有**清空/重建 `VisibleActors`。

## 3. 为什么当前不表现为帧差异

`spacemove` 这一批的帧全部走：
- `SendMsg` / `SendDefMessage` 类**自见**帧（633/634/54/766/11029/807/640）；
- `SendRefMsg` 类广播 —— 而 Rust 的 `SendRefMsg` 冷路径**已不依赖观察者的 `visible_actors`**
  （见 `mir2-rs/bins/game-svr/src/lib.rs` 中 `SendRefMsg` 相关注释）。

⇒ 本批 golden 能逐帧对上；**但这不是"差异不存在"，只是"这批的观测面看不到它"**。

## 4. 风险面

瞬移**之后**任何依赖 `VisibleActors` 的逻辑会带**旧视野**跑，例如：

- `MagPushArround`（`src/M2Server/Magic/MagicManager.cs:27`）——按 `VisibleActors` 列表序遍历；
- 其它按"可见集"选择收件人的路径（若将来有 `SendRefMsg` 之外的视野消费者）。

表现形态会是"瞬移后仍能推到旧位置附近的对象"，而 oracle 此时视野已空（要等到下一次 AOI 重建才有对象）。

## 5. 移除条件（本卡关闭的判据）

1. 在换图簇里补 **`VisibleActors.Clear()`**（与 oracle 同位置：`DeleteFromMap` 之后、改坐标之前）；
2. **重建**：按 Rust 现有 AOI 机制在新位置重建（或明确 oracle 在下一次 `SearchViewRange` 才重建，Rust 同拍）；
3. **观测面**：一个能读"瞬移后视野集合"的探针（oracle 侧 + Rust 侧同口径）；
4. **牙**：删掉清空点 ⇒ 瞬移后的收件人/推挤面红（红在该支自己的断言上）；
5. 跑既有 magic/world 比较器确认零漂移（`SpaceMove` 是共用路径，爆炸半径覆盖移动/跨图批次）。

## 6. 边界

- 本卡**不**包含实现；实现另开卡并走 B 类（共享路径）评审。
- 与 `docs/issues/magic-push-batch-impl.md` 的关系：那批**不覆盖**瞬移后的视野状态，见本卡。
