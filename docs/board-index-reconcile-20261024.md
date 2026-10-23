# 「board 未列出」5 张的对账（`openmir2-board-index-reconcile`）

- 卡：`openmir2-board-index-reconcile`（tier=L）；分支 `card/openmir2-board-index-reconcile`（base master `2810298e9`）
- 写集：`.walgit/board.toml`、`tools/goal/`、`docs/` —— **本卡未改 `board.toml`**（理由见 §4），只出对账与逐张说明。

## 1. 名单（现取）

```sh
python -I tools/goal/goal_gate.py --json      # 条件②「其它需人工 5」
```

快照戳：`generated_ts` 见 `/e/tmp/board-before.json`；条件② detail 原文：
`… 其它需人工 5 …`，5 张为：
`openmir2-m3-monster-ai-tick`、`openmir2-client-e2e-enter-gaps`、`openmir2-conformance-rng-nondeterminism`、
`openmir2-menu-ok-unify-pickwire`、`openmir2-ybdeal-frames`。

## 2. 逐张核查（线程可读、签名全 verified）

| 卡 | 条目 | 最后一条 `status` | 其后的条目 | `ls` 有 | `board` 有 |
|---|---:|---|---|---|---|
| `m3-monster-ai-tick` | 36 | `in-progress`（`ad90aec7`，258h） | review `44d3e347` → merge_result `35e78e47` | ✅ | ❌ |
| `client-e2e-enter-gaps` | 549 | `open`（`2eff7104`，246h） | patch `8fe0e463` → merge_result `9d1510e1` | ✅ | ❌ |
| `conformance-rng-nondeterminism` | 11 | **无任何 status** | issue→patch→merge_result→comment | ✅ | ❌ |
| `menu-ok-unify-pickwire` | 10 | `in-progress`（`c5c8e750`，16h） | merge_result `db866641` → comment `74783a02` | ✅ | ❌ |
| `ybdeal-frames` | 6 | `in-progress`（`7f9e48dc`，15h） | review `b1f716d5` → merge_result `15bc3ebf` | ✅ | ❌ |

旁证：`walgit collab ls` 共 **472** 个 thread id，5 个 id 都在；`board --format json` 共 **461** 张卡，5 个都不在。
另外抽 40 张**已在板上**的卡看 head kind：`status` 27 / `comment` 12 / `card_claim` 1
⇒「head 是 comment 就不显示」不成立；**差异只在这 5 张都有 `merge_result` 头或从未有 status**。
（`board.toml` 的列只按 `status` 值匹配，见文件内注释与 6 个 `[[column]]`。）

## 3. 定性与处置（选 ②：卡的状态该补）

- 4 张（monster-ai / client-e2e / menu-ok / ybdeal）：**已 merge、但 merge_result 之后没有新的 `status`**。
  看板投影对它们不落列 ⇒ 需要 **owner/coordinator 追加一条签名 `status`** 反映现状（`closed` 或带窗口项的 `in-progress`）。
- 1 张（`conformance-rng-nondeterminism`）：**从未有过 status**（issue→patch→merge_result→comment）⇒ 同样需要一条签名 `status`。
- 已在每张卡留签名 comment（不擅自改他人 status）：
  `4d7bf3c2`（monster-ai）、`047a864c`（client-e2e）、`23257c1f`（rng）、`c05b6dd5`（menu-ok）、`64c671d2`（ybdeal）。

## 4. 为什么**未**改 `board.toml`

1. 需要新增的规则形如「head 是 `merge_result` 或线程无 status ⇒ 进某一列」，但 `board.toml` 现有 schema 只有
   `[[column]] status=<值>` 六个列，**没有按条目 kind / merge_result 匹配的字段**；硬改要么无效、要么把别的列改乱
   （卡面要求「其它列没有变化」的实测对照，用当前 schema 做不到）。
2. 放宽视图属于**看板口径变更**（`AGENTS.md`：改 `board.toml` 走提交 + 评审）。本卡级别不足以定义新语义
   （例如「merge_result 之后无 status 的卡应落在哪一列」——`已完成`？`待窗口` 不存在），应由 owner 决策。
3. 本卡判据允许「或每张剩余都有签名 comment 说明」⇒ 采取该支。

**建议（给 owner）**：给 `board.toml` 增加一门 `[[column]] status = "merged"`（或 `merge_result = true`），
并约定「merge_result 之后无 status ⇒ 按 merged 落列」；届时 coordinator 合并时补一条 `status: merged` 即可
让投影自洽，而不必依赖每张卡的人工补登。

## 5. 前后对比

- 改前：条件② `其它需人工 5`；`board` 461 张、5 张不在板上。
- 改后（本卡交付时）：**计数仍为 5**——因为本轮只补了说明、没有他人 status；`goal_gate` 不读 comment。
  按卡面判据的「或」支：5 张**每张都有签名 comment** 说清谁该补、补什么（§3）。
- 若 owner 按 §3 各补一条 status ⇒ 5 张会重新出现在看板（`ls` 已含），届时计数自然归零。

## 6. 残留物

- `E:/tmp/board-before.json`、`E:/tmp/board-now.json`（`goal_gate --json` 与 `board --format json` 快照）。
