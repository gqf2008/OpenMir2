# B2/M4 审查发现收口批次报告（广播 MsgType + AutoGetExp 崩溃面登记）

> 卡片：`openmir2-b2-script-dispatch`（owner `openmir2-worker-3`，worktree `b2`，分支 `worktree-b-script-engine`）
> 触发：本会话按 `brief-b2-handlers.md` 复核发现——**卡的 DoD「语料派发目标未实现数为 0」在本批开工前已由
> `ea1b14ef`/`0ac357c3`/`8ff31f57`/`dcafa02b` 达成并交付**（`coverage-var-after.txt`：106 已实现 / 0 未实现）。
> 因此本批不再重复补 handler，只收口 reviewer-1 对动作批次 1 的 2 条 **minor** 发现（`d938a958` 的 `findings`）。

## 1. 一句话结论

DoD 复跑仍为 **106 已实现 / 0 未实现（11829 次调用，`--require-implemented` exit 0）**；收口 reviewer-1 的两条
minor：① 广播钩子贯穿 `MsgType`（LINE MSG 0-3 = `System`，其余 = `Say`）并升成单测机判；② 空/单字符
`sParam3` 的「C# 崩 / Rust 不崩」按纪律登记 `whitelist B-108` 并补单测钉住。全部门禁绿，
**未自行 merge、未签 review**。

## 2. 改动面（本批 `28041146`）

| 文件 | 改动 |
| --- | --- |
| `crates/script/src/engine/mod.rs` | `ScriptPlayer::world_broadcast(msg, msg_type)`：新增 `msg_type` 形参（世界适配层据此选广播类型） |
| `crates/script/src/engine/actions.rs` | 新增 `MSG_TYPE_SYSTEM = 0`；`ActionOfLineMsg` 0-3 传 `System`；其余 10 个 `SendBroadCastMsg` 落点传 `MSG_TYPE_SAY`（与 C# `ExecutionProcessingSys.cs` 的 16 处 `SendBroadCastMsg` 一一对应：4 System + 12 Say，其中 2 处在 Rust 侧对应动作尚未落广播钩子） |
| `crates/script/tests/engine_tests.rs` | MockPlayer 事件改记 `broadcast {msg_type} {msg}`；`line_msg_modes_and_error` 断言 0/2/3 = `System`；`auto_get_exp_and_offline` 补空 `sParam3` 分支（落 `false`、不崩） |
| `tests/parity/whitelist.md` | 新增 `B-108` |

## 3. 两条审查发现的处置（逐条对应 `d938a958`）

### minor#1 `ActionOfLineMsg` 的 MsgType 未机判

- C#：0-3 = `SendBroadCastMsg(sMsg, MsgType.System)`（`ExecutionProcessingSys.cs:3170-3179`），
  5-7 = `SysMsg(..., MsgType.Say)`（已由 `sys {color} {msg_type}` 断言为 3）。
- 原 Rust 钩子 `world_broadcast(&mut self, msg: &str)` 不带类型，测试无法机判 ⇒ 世界适配层映射错抓不到。
- 处置：钩子加 `msg_type`；`ActionOfLineMsg` 0-3 传 `MSG_TYPE_SYSTEM`；MockPlayer 记 `broadcast {msg_type} {msg}`，
  单测断言 `broadcast 0 …`。**非 LINE MSG 的 10 处广播**（C# 全部 `MsgType.Say`）一并传 `MSG_TYPE_SAY`，
  使「System vs Say」在这条接口上可机判。

### minor#2 空/单字符 `sParam3` 的崩溃面未登记

- C#：`questActionInfo.sParam3[1] == '1'`（`ExecutionProcessingSys.cs:2696`）对空/单字符抛
  `IndexOutOfRangeException`（被 NPC 层 catch 吞掉 ⇒ 玩家侧「点了没反应」）。
- Rust：`chars().nth(1) == Some('1')` 落 `false`、不崩。
- 处置：新增 `whitelist B-108`（差异点 / 为什么无害 / 谁审的），并补单测把 Rust 落点钉住。
  这是**刻意不复刻崩溃**（S2 崩溃面），与 B-107 同口径。

## 4. 门禁证据（原始输出见 `review-findings-gates.txt`）

| 门禁 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | clean（exit 0） |
| `cargo test --workspace --offline` | 全部 test target `ok`，**failed 总数 = 0** |
| `cargo run -q -p msgcodegen -- --check` | 604 条常量与 C# 源一致 |
| `run_dispatch_regression.ps1 -RequireRustFlipped` | 5 PASS / 0 FAIL，gate GREEN |
| 覆盖报告 `parse-stats … --require-implemented` | **106 已实现 / 0 未实现**（`coverage-review-findings-after.txt`），exit 0 |
| `python -I tools/flow-diff.py tests/parity/flow-cases.json` | **3 一致 / 0 不一致 / 1 exempt-OK**（exit 0，`flow-diff-review-findings.txt`） |

- 证据四件套：命令见上；输出 = `review-findings-gates.txt` / `coverage-review-findings-after.txt` /
  `flow-diff-review-findings.txt`；路径 = `mir2-rs/tests/parity/evidence/B2/`；
  hash = 代码批 `28041146`（本报告随其后一条归档提交）。

## 5. 残留物清单（诚实登记）

1. **本会话只收口 minor，不改 oracle**：`src/**` 未动，客户端未动，未 merge、未签 review。
2. **var 批次仍待独立审查**：patch `f71fa42b @dcafa02b` 的 `status: needs-review` 尚无不同 principal 的 `review`；
   本批在其之上，须与该批一并由 reviewer 复核。
3. **`world_broadcast` 的 `msg_type` 目前无人消费**：世界适配层未接入（默认可观测替身仅测试用），
   与 `var-batch-report.md §5` 的世界面残留同源；接世界时按本批传参落 `System`/`Say`。
4. 临时物：`E:/tmp/openmir2-watch/{b2b-coverage,b2b-flow-diff,b2b-dispatch-gate}.txt` 为本批原始输出，
   已归档为上述证据文件，仓库外临时文件不再引用。
