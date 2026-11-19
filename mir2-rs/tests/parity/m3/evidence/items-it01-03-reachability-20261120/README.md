# IT-01/IT-03 可达性整改 —— 帧级 diff 分类器（QA 可复跑）

`framediff.py` 对比**旧夹具**（可达性整改前的 `eat_vectors.json`，如 M56 `07edcd30b` 版本）与**重生成后夹具**，
逐 case 比 `immediate_frames / notices / pumped / added_items`（全字段含 priority/late）+ `wabil(后)` +
`wabil_before(输入元数据)`，把每个 case 归入四桶：

- **consumed**：该例 `max_hp/max_mp` 被消费（`shape1_fixed/clamp`、`shape3_percent`）⇒ 帧面变化属"被消费字段变了，应同步"。
- **52tail/state**：仅 `wabil(后)` 的 `MaxHP/MaxWeight/DC…` 变；若该例发 `SM_ABILITY(52)`，其尾即带真实 WAbil ⇒ "更真实非回归"。
  **硬条件**：该 `wabil` 字段必须**确被比较器断言**，且 **Rust 侧同值**（cargo 对该 case 绿）——两者缺一即**不得**入本桶。
- **52tail-uncompared**（子桶）：某 case 的 `wabil(后)` 变了，但**该字段没有比较器断言覆盖**（该 case 不比 52/wabil，或只比子集）⇒ cargo 绿**只证没红、不证相等**。写清"Rust 侧该字段无断言、本桶不作一致性证据"，点名列出，**不得**并进 `52tail/state`。若 items 无任何 case 对拍 52 尾，则"52 更真实"整体**未验证**，如实写。

**比较器覆盖事实（供分类）**：`useitem::tests::matches_oracle_eat_fixture` 末尾 `assert_eq!(state.wabil, ability(&expect["wabil"]))` 断言**整个 wabil 结构体相等**（全字段）⇒ 该比较器覆盖的 case，wabil 字段变化**已被断言**（cargo 绿即相等，入 `52tail/state`）。`it08_levelup_host_parity`（host_levelup 例）只断言 `max_hp/max_mp/hp/mp/dc` 子集 ⇒ 若该例其余 wabil 字段(max_weight/ac/mac…)变而无断言，入 `52tail-uncompared`。**实现时逐 case 核此覆盖，实跑为准。**
- **metadata**：仅 `wabil_before` 变、**帧面 0 差异** ⇒ "仅夹具元数据变、帧面不变"。
- **UNEXPLAINED**：非消费例却帧面变 ⇒ 回归嫌疑，逐条列 + **停下报 opc-07**，不擅自归无害。

用法：
```
python /tmp/old_eat_vectors.json /path/new_eat_vectors.json
# 旧夹具可取：git show <M56>:mir2-rs/tests/parity/m3/fixtures/eat_vectors.json > /tmp/old_eat_vectors.json
```
`framediff.py` 只判 oracle 侧夹具差；**Rust 一致性**由 `cargo test -p game-svr --lib` 绿来确认（任何被分类 case 若 cargo 红 ⇒ 按 UNEXPLAINED 处理并停下）。
