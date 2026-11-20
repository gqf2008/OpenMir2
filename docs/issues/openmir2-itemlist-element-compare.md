# openmir2-itemlist-element-compare

> 状态：**已实现（2026-11-20，`card/itemlist-elements`；第二方待签）**。
> 来源：IT-17 复评的"记录而不比"审计；**卡面前提已纠正**（见 §0 表）——IT-16 的吃物品路径
> 早已逐件比，真缺口在 `readbook` 与 `eat_move` 三处。

## 0. 现状盘点（master `da63bae28`，动作前）

| 位置 | 动作前比了什么 | 漏了什么（夹具已记录） |
| --- | --- | --- |
| `readbook.rs`（IT-17a 对拍器，`run_readbook_parity`） | **只比件数** | 元素全漏：`make_index/index/dura/dura_max`（夹具记录这 4 字段，**无 `desc`**） |
| `eat_move.rs`（IT-09/10/11 `run_move_parity`） | 件数 + **仅首件 `make_index`**（且仅 golden 非空时） | `index/dura/dura_max/desc` 以及"非首件"全漏（夹具记录完整 5 字段） |
| `eat_move.rs`（IT-13 对照器） | **只比件数** | 元素全漏（夹具记录完整 5 字段） |
| `useitem.rs`（IT-16 吃物品路径） | **已逐件比**：前段整 `BagItem` 含 `make_index`；解包尾段排除墙钟 `make_index` 后比 `(index,dura,dura_max,desc)` | **无缺口**（本卡以此为写法样板，**不动**） |

"记录而不比"比"未覆盖"更危险：夹具里明明有值，看起来像在比。

## 1. 实现（2026-11-20）

- `eat_move.rs` 两处：golden `item_list` → `Vec<BagItem>`（新 helper `golden_bag_items`，5 字段全取），
  **整 `Vec` 相等**（这些路径无解包、无墙钟 `make_index` 生成 ⇒ 不需要排除项）。
- `readbook.rs`：逐件比 `(make_index, index, dura, dura_max)`；**夹具未记录 `desc` ⇒ 该字段本卡不覆盖**
  （若要覆盖，先扩 `readbook-probe` 的记录面，另开）。
- **IT-16 未动**（其比较器已是标准形态）。

## 2. 牙（§50：三处各一颗，且必须证明"只红对应的那一条"）

每颗都把**该处期望的 `dura` + 1**（`MUTATION` 标记，§31 还原后 sha 相等、`grep MUTATION`=0）：

| 牙 | 变异点 | 红 | 同变异下仍绿（隔离证明） |
| --- | --- | --- | --- |
| T1 readbook | 期望 `dura+1` | `it17a_*`：6 条里 **4 failed**（带件的 4 条门） | `it09_` 1 passed |
| T2 eat_move 对拍器（IT-09/10/11） | `want_items[0].dura+1` | `it10_` **2 failed**（唯一保留件的例 `eat_fail_shape2_norandommap_no_eat`；it09/it11 的例均已吃空 ⇒ `first_mut` 不触发） | `it13_` 6 passed、`it17a` 6 passed |
| T3 eat_move IT-13 对拍器 | `want_items[0].dura+1` | `it13_` **3 failed**（`eat_fail_shape5_no_guild` 等保留件的例） | `it09_` 1 passed、`it17a` 6 passed |

⇒ 三处**彼此独立**（无共享比较函数/状态），"一条用例钉住 A 支不代表 B 支"的反例在 T2 的 it10 上可见
（同组内只有"有件"的例能咬）。

## 3. 门

- `cargo test -p game-svr --lib -- it09_/it10_/it11_/it13_/it17a` → 1/3/6/6/6 全 passed，rc=0；
- 全量 `cargo test -p game-svr --lib` → `running 2228`，**2155 passed / 0 failed / 73 ignored**，rc=0；
- `cargo fmt --all -- --check` rc=0；`clippy --all-targets` 待窗口（未起）。

## 4. 边界

- 不改 `use_item` / `readbook` / `eat_move` 语义；只改测试侧比较器。
- readbook 的 `desc` 字段不覆盖（夹具未记录）；`magic_arr_*`（17b）与 `char_status/hit_speed`
  （经 657 payload 间接比）不在本卡。
