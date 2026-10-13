# 改坏必红索引 —— M3⑦-实现 2（组队状态机）

每条 = 生产代码 mutation → 跑指定用例 → 必须 FAILED（rc=101）→ `cp` 还原 → 同用例恢复 ok。
原始件为逐条命令 + 输出。

| 编号 | mutation（位置） | 期望必红用例 | 原始件 |
| --- | --- | --- | --- |
| RC-1 | `crates/world/src/party.rs` `party_add_group_member` 的 `>` 改 `>=`（严格大于 → 大于等于） | `add_member_uses_strict_greater_than_max` | [`RC1-strict-greater-red.txt`](RC1-strict-greater-red.txt) |
| RC-2 | `crates/world/src/party.rs` `Emitter::finish` 删掉 `self.queued.sort_by_key(...)`（丢出队序） | `del_member_regular_keeps_leader_with_cancel_and_delmem_ok` | [`RC2-queue-priority-red.txt`](RC2-queue-priority-red.txt) |
| RC-3 | `bins/game-svr/src/smframe.rs` `sm_groupmembers` 尾 `&edcode::encode(members_text)` 改 `members_text`（丢 `EncodeString`） | `party_cm_dispatch_emits_oracle_frames_on_both_connections` | [`RC3-encode-red.txt`](RC3-encode-red.txt) |

三条均为 FAILED rc=101；还原后 `party_vectors` 12/12、`game-svr` 组队 2/2 恢复全绿。
