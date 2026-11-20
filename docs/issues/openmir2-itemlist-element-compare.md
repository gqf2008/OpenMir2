# openmir2-itemlist-element-compare

> 状态：**待认领**（2026-11-20 立卡，来源 = IT-17 复评的"记录而不比"审计）。
> 关联：IT-17a（`readbook.rs` 比较器）与 **IT-16 吃物品路径**（同口径比较器）。

## 0. 问题

两处对拍器对 `expect.item_list` **只比件数**，元素字段**记录而不比**：

- `bins/game-svr/src/readbook.rs:564-566`（IT-17a）：失败支"原件保留"只由件数 + `MagicList` 不变间接证明；
- IT-16 吃物品路径的同口径比较器：凡"失败不吃/保留原件"的用例都只证件数。

**"记录而不比"比"未覆盖"更危险**：夹具里明明有值，看起来像在比。当前 readbook 夹具的
`item_list` 元素记录字段 = `make_index / index / dura / dura_max`（无 `desc`）。

## 1. 判据

- 两处比较器**同时**改为逐元素比 `(make_index, index, dura, dura_max)`（与夹具已记录字段同集）；
- **两处一起改、一起验**：比较器是共享件，只改一处会让另一条的"保留原件"面出现假绿；
- 若将来要用 `desc`：先补探针记录面再比，不在本卡臆造。

## 2. 验收

- 反例（必须咬）：把 golden 的 `item_list[0].index` 改一个值 ⇒ 对应用例红（readbook 与 eat 两侧**各一**）；
- 探针若需扩展记录：重出夹具后证"可比重叠面逐字节不变"；
- 全量 `cargo test -p game-svr --lib` 三数（新增/变化的用例要解释）。

## 3. 边界

- 不改 `use_item` / `readbook` 语义；只改测试侧比较器（必要时扩探针记录面）。
- 不在本卡修其他"记录而不比"键（`magic_arr_*` 属 17b；`char_status/hit_speed` 经 657 payload 已间接比）。
