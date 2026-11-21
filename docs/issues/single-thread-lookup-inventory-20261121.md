# 单线程容量：查找点清单与排序（2026-11-21，只读产出）

> 卡：`card/lookup-inventory`（基线 master `341dbb149`；口径 A `docs/acceptance-tier-a-20261121.md`：
> 内部结构可改、**可见输出不变**）。本文件**不改代码、不跑构建**，只列候选点与风险。
>
> **统计口径**：每行 `来源` 列标明 `读代码`（本文件作者读源码得出）或 `需实测`（本次未量化）。
> 两者不得混写；后文排序里的收益量级均为 `读代码` 的**结构性判断**，不是测量值。
>
> **锚**：高 churn 文件一律用**符号锚**（`文件::符号`），不用行号。

## 1. 清单

| # | 结构（定义） | 现类型/查找方式 | 典型调用点（符号） | 每次复杂度 | 每次事件调用次数（量级） | 来源 |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | `OnlineRegistry`（`bins/game-svr/src/lib.rs::OnlineRegistry`，`Vec<OnlineEntry>`）——**按 actor_id 查** | `online.iter().find(\|e\| e.actor_id == id)`（全仓 66 处） | `send_ref_msg_to_targets`、`broadcast_ref_msg_opt`（收件人→socket 路由）、各 `drain_*` 下行、`handle_*_cmd` 的 sent 回执路由 | O(n) | 每条出站帧路径一次；扇出时 × 收件人数（n≈2,800 时单帧 ~2.8k 比较） | 读代码（66 处为 `grep` 计数） |
| L2 | 同上——**按 chr_name 查** | `online.iter().find(\|e\| e.chr_name == …)`（44 处） | 所有 `handle_*_cmd` 的入口定位、`cheat/claim` 等 | O(n) | 每条入站命令 1–3 次（入口 + 下游） | 读代码（44 处） |
| L3 | 同上——**登记/摘除** | `online_set`（`iter_mut().find` 后 push）、`online_take`（`position` + `Vec::remove`） | 进世界、断线、顶号 | O(n)（`remove` 另有 O(n) 搬移） | 每会话生命周期 2 次 | 读代码 |
| L4 | 同上——**每网关顶号** | `online_find_for_kick`：按 `gate_idx` 分桶、每桶取登记序首个匹配 | 账号级顶号（`TCPNetChannel.KickUser` 等价物） | O(n) 每网关 | 每次顶号 × 网关数 | 读代码 |
| L5 | `DisplayTable`（`bins/game-svr/src/downlink.rs::DisplayTable`） | **`Vec<(id, display)>` + `HashMap<i32, usize> by_id`**，`get` 走哈希 | 所有 AOI/帧下行取展示行 | O(1) | 每帧一次 | 读代码（**如别处文档称其 O(n)，以代码为准：文档过时**） |
| L6 | `ground_sessions` / `sessions`（`HashMap<String, _>`） | 哈希直查 | 会话态读写、断线清理 | O(1) | 每命令 1 次 | 读代码 |
| L7 | `guild_recipients` → `guild_cmd::find_play_object` | 闭包内 `online.iter().find` + 每次调用 `to_upper` 分配 | 行会通知/断线生命周期（`send_guild_notices` 等） | O(n) × 收件人 | 每行会事件 × 在线成员数（最坏 O(n²) 级） | 读代码 |
| L8 | `map_cell_object_ids`（`crates/world-bevy/src/world.rs::map_cell_object_ids`） | 逐格扫 `ObjList` 收集 id | `SimMapEvents` 段、格子事件 | O(格子对象数) | 每事件格一次；格子对象数通常个位数 | 读代码；**需实测**（密度 17.7 下格内数量分布） |
| L9 | `EntityStore::get`（bevy ECS 内核索引） | 哈希 | 全场实体取用 | O(1) | 每处 1 次 | 读代码 |

## 2. 判定列

| # | ① 能否改保序索引（`Vec` + `HashMap<K, usize>`）？ | ② 改后是否可能改变迭代顺序（红线） | ③ 现成测试能否抓住顺序变化 |
| --- | --- | --- | --- |
| L1 | **能**：`HashMap<i32, usize>` 旁挂，`Vec` 仍是唯一真源；写点在 L3 的三处 | **否**（索引只加速 `get`，不动 `Vec` 序）；**唯一风险**：若 `remove` 用 `swap_remove` 会变序 ⇒ 必须用 `Vec::remove` + 索引重建（O(n) 但只发生在摘除/顶号，不在热路径） | 部分：AOI/帧序 golden（`b1_golden_drain_assert` 等）能抓跨接收者帧序；**缺**直接断言"登记序"的用例 ⇒ 若改坏 L4 语义，只有 `account_kick_targets_every_gateway_once` 抓 |
| L2 | **能**：`HashMap<String, usize>`（`chr_name` 唯一，`online_set` 已按它去重） | **否**（同上） | 同上 |
| L3 | 保持 `Vec` + 两个索引同步 | **是（需显式保序）**：`online_take` 现在用 `Vec::remove` 保序；索引重建必须照做 | `account_kick_targets_every_gateway_once`、断线/重连用例 |
| L4 | 查找可走索引，但**"每网关首个"语义依赖登记序** | **是**：改成"全局首个"会改变可见行为（已登记 O-3 裁决） ⇒ 索引后仍必须按 `gate_idx` 分桶取**最小下标** | 有：`account_kick_targets_every_gateway_once` |
| L5 | 无需改（已是索引） | — | `DisplayTable` 自测（`downlink` 测试段） |
| L6 | 无需改 | 迭代序**不得**参与逻辑（现代码未用其迭代序；新增遍历时必须用显式排序键） | 无直接用例 ⇒ 新增遍历要自带断言 |
| L7 | 能：随 L1/L2 索引后 `find_play_object` 变 O(1)；另可缓存 `to_upper` 结果 | 否（闭包只做 `Option` 返回） | 行会通知帧序用例（`guild_runtime` 测试段） |
| L8 | 可选：格子对象一般极少，改索引收益存疑 | 若改遍历顺序 ⇒ 格内事件顺序变 | `SimMapEvents` 段用例 |

## 3. 排序（理论收益 ÷ 风险）

1. **L1+L2+L3：`OnlineRegistry` 旁挂 `actor_id` / `chr_name` 两个 `HashMap<_, usize>`**（一个结构、一次改动）。
   - 收益：消除 66（按 id）+ 44（按名）处 O(n)；出站扇出与入站命令全走这里，n 从 ~986 升到 ~2,800 时这是**首要放大器**；结构改动小（`Vec` 仍是真源）。
   - 风险：中——写点只 3 处（`online_set`/`online_take`/顶号），但**必须保住 `Vec::remove` 的保序语义**；L4 的"每网关首个"要按最小下标取。
   - 测试缺口：缺"登记序"直接断言 ⇒ 建议先补一条（不改逻辑，只加断言）再动结构。
2. **L7：`guild_recipients`/`find_play_object`**。
   - 收益：行会事件是"每事件 × 成员"的乘性 O(n)，且每次 `to_upper` 分配；跟随 L1/L2 后基本归零。
   - 风险：低（纯查找替换），但要把 `to_upper` 的语义（大小写不敏感）保留。
3. **L8：`map_cell_object_ids`**。
   - 收益：**需实测**——若密度 17.7 下格内对象仍是 O(1) 级，则收益低、不值得动；动它反而有顺序风险。
   - 建议：先量化再决定（`43` 的实测口径）。

**不建议动**：L5（已是索引）、L6（已是哈希）、`EntityStore`（ECS 内核索引）。

## 4. 备注（文档过时/口径）

- **文档过时**：`DisplayTable` 已经是 `Vec + HashMap` 索引；任何把它的 `get` 描述成 O(n) 的旧文字以本表 L5 为准。
- **需实测**：L8 的格内对象分布、以及"每 tick 各 `online.iter()` 实际次数"（可用 tick profile / 计数器在 `43` 的批里量化）——本文件不给测量值。
- **红线复述**：任何"保序索引"改动只能加速 `get`，不得让 `HashMap` 迭代序进入逻辑；摘除必须维持 `Vec` 原序（`Vec::remove`，不用 `swap_remove`）。

## 5. 实测：`simmonsterai_bench` **不经过** `OnlineRegistry`（2026-11-21）

`mir2-rs/bins/game-svr/src/simmonsterai_bench.rs` 全文件**没有** `online` / `OnlineRegistry` /
`handle_say` / `send_ref_msg_to_targets` 引用；`run_arm` 只 `build_world` + `world.tick()`。
所谓"客户端 500/1000 档"是 `race=0` 的实体，不是在线表 ⇒ 用它跑 L1/L2 索引前后两臂，
测到的是**同一份代码**。

实测（同机、同 target、debug、每臂 3 次，口径
`MIR2_SIMMON_BENCH=1 MIR2_SIMMON_CLIENT_TIERS=500,1000 MIR2_SIMMON_CLIENT_ONLY=1 MIR2_SIMMON_TICKS=100`）：

| 档（ms/tick，median） | 无索引 | 有索引 | 结论 |
| --- | --- | --- | --- |
| player-only 500 | 20.435 | 22.001 | 噪声 |
| player-only 1000 | 40.301 | 52.196 | 噪声（单次抖动达 34%） |
| mixed 500 | 172.085 | 183.731 | 噪声 |
| mixed 1000 | 222.870 | 243.929 | 噪声 |

**用途边界**：该 bench **不能**作为 L1/L2 索引的收益证据。端到端收益需
`sim_loop` 每 tick 的 `online.iter().find` 调用次数 × 单次耗时探针（另立卡）；
算法级证据见 `online_lookup_micro_index_vs_linear` 微基准（n=986/2800，线性 vs 索引，≥5× 才算过）。
