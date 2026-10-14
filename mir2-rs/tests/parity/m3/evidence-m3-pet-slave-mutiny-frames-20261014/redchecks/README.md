# redchecks/ —— 改坏必红（RC1–RC5）

卡：`openmir2-m3-pet-slave-masterdie-wire`（rework：reviewer-6 F-1）；作者 `openmir2-worker-20`。

口径：**改 → 跑 → 还原**；还原用 `git checkout --` 后以 `git status --porcelain` 实现文件干净 +
聚焦用例全绿机器核对。变异脚本 = [`mutate.py`](mutate.py)
（`python -I mutate.py <rc1|rc2|rc3|rc4|rc5>`，断言 anchor 唯一；相对返工提交 `ecd455765`）。

> 本目录各 `RC*-mutated.txt` 已在**本轮前移 head `5e932b4eb`**（base 3db79cae0）上复跑，读数不变
> （均 `rc=101`）；`restored-green.txt` 亦在该 head 上重生成。

| 编号 | 变异 | 应红的用例 | 实读 |
| --- | --- | --- | --- |
| RC1 | `smframe.rs`：`sm_changenamecolor` 的 `Param` 由 `u16::from(name_color)` 改 `0` | `smframe::tests::sm_changenamecolor_header_only` | **rc=101** · 0 passed 1 FAILED |
| RC2 | `main.rs`：drain 退回「所有色循环 + 所有名循环」（同优先级=入队序，F-1 前错误） | `tests::slave_mutiny_downlink_follows_msgqueue_heap_order` | **rc=101** · 0 passed 1 FAILED |
| RC3 | `main.rs`：丢掉名帧（只入队色帧） | 两条 `slave_mutiny_downlink_*` | **rc=101** · 0 passed 2 FAILED |
| RC4 | `msgqueue.rs`：`LOG2_ARITY` 2→1（4 叉堆改成二叉） | `msgqueue::tests::matches_dotnet_priority_queue_dequeue_order`（n=3 起分叉） | **rc=101** · 1 passed 1 FAILED |
| RC5 | `main.rs`：色帧优先级抬到 `255`（> 名 165） | 两条 `slave_mutiny_downlink_*` | **rc=101** · 0 passed 2 FAILED |

还原后重跑（[`restored-green.txt`](restored-green.txt)）：全绿 rc=0；`git status --porcelain` 实现文件干净。

> **RC2 是本轮 F-1 的针对性牙**：它把「堆出队序」换回「按优先级稳定排序（入队序）」，正好打红
> `...follows_msgqueue_heap_order`（名帧 `名(s2),名(s1)` vs oracle `名(s1),名(s2)`），
> 而单宝宝的 `...emits_namecolor_then_username` 在 RC2 下仍绿 —— 两条用例分工明确。
> **RC4** 证明「.NET 不是二叉堆而是 4 叉堆」这一细节可观测（n=3 出队序分叉）。
