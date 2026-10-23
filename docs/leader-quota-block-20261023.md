# 配额中断与恢复手册（2026-10-23 11:28）

## 发生了什么事

11:28–11:29，**全部子代理**同时收到 `429 rate_limit`：

```
[1308][已达到 5 小时的使用上限。您的限额将在 2026-10-23 14:01:22 重置。]
model sent to the API: glm-5.3-flash
```

即：**子代理所用的模型配额耗尽，14:01:22 前无法再起任何 subagent**。Leader 自身的会话（Opus）不受影响，
仍可做合并、记账、读代码、写文档。

## 中断时在飞的 4 条车道（都有未落盘的验证成果）

| agent | 卡 | 中断时的状态 | 恢复成本 |
|---|---|---|---|
| reviewer-3 | `openmir2-weapon-durability-frames` | 「所有检查已完成，正在撰写复审」——**验证做完、review 未发** | 重发即可（结论已在上下文里） |
| reviewer-10 | `openmir2-dualrun-window-burndown` | 「三项牙全部 fail-closed 确认，准备 767 活机复跑」——验证做完、**review 未发** | 重发即可；767 复跑可改由 Leader 或新 agent 做 |
| reviewer-6 | `openmir2-sm-reconnect-wiring` | 刚恢复到「复审 I-1 金种子三路法」 | 重新起复审 |
| worker-52 | `openmir2-monster-showhp-53-broadcast` | 刚收到「补 R3-1a 用例」指令，未开工 | 重新派 |

## 已确认落地、不受影响的

- 22 张卡已合并进 master（`fc9a409c7` 为最后一次合并）；工作树干净、三/六道门在合并树上全绿。
- 所有 R 卡窗口项、残余、L 卡收口状态都已签名入账（`merge_result` / `status`）。
- 队列文档 `docs/leader-queue-20261023.md` 与终点机判 `tools/goal/goal_gate.py` 都在 master。

## 恢复步骤（14:01:22 之后）

1. `git fetch walgit '+refs/collab/inbox/*:refs/collab/inbox/*'` + `walgit collab board --format json` 对齐；
2. **先恢复两条「验证已完成只差发 review」的车道**（weapon-durability、burndown）——它们是纯写结论，成本最低、收益最高；
3. 再恢复 sm-reconnect 复审与 showhp53 用例补丁；
4. 之后按常规循环（合并 > 派活 > 拆卡）。

## 教训

- **长时间无人值守循环要预留配额余量**：本轮从 08:00 起持续派 agent，累计 30+ 个子代理，把 5 小时配额打满。
  后续应按「每小时 agent 数上限」节流，或把纯记账/纯文档工作留给 Leader 自己做（不消耗子代理配额）。
- **「验证做完但结论没发」是最贵的失败态**：复审的牙测试要跑全量 `cargo test`（5–15 分钟），
  建议复审流程改为「先发一版结论，再补细节」，避免整段验证白费。

## 附：条件④ 的「clippy 红」是资源竞争的假红（已澄清）

中断后 `goal_gate.py` 快速模式报 `条件4 clippy rc=101（8.6s）`。在无并发 cargo 的情况下重跑：

```
cargo clippy --workspace --all-targets -- -D warnings  ⇒ rc=0
```

⇒ 那条红是**当时多个已死 agent 的 cargo 进程争锁/半途被杀**造成的，**master 本身是绿的**。
教训：`goal_gate` 跑门禁前应先确认无并发 cargo（或加 `--locked`/独立 target），否则会把资源竞争误报成代码缺陷。
