# 任务：B2/M4 —— 派发目标批量补 handler（worker-3）

仓库/工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\b2`（分支 `worktree-b-script-engine`）。
卡：`openmir2-b2-script-dispatch`，owner 身份 `openmir2-worker-3`（worktree 已 join）。

## 已有（本卡最近进展）
Rust 位移已同批翻转（`parser.rs` field_index）；已实现 `GotoLableTakeItem`/`ActionOfMapMove`/`ActionOfGiveItem`；
**flow-diff 3 一致 / 0 不一致 / 1 exempt-OK（exit 0）**，必须保持不回归。

## 本轮目标
按「未实现清单」批量补 handler，直到**语料用到的派发目标未实现数为 0**：
- 三态表见 `crates/script/src/engine/dispatch.rs`：`IMPLEMENTED_CONDITIONS` / `IMPLEMENTED_EXECUTIONS`。
- 语义照 C# `src/Modules/ScriptEngine/Processings/{ConditionProcessingSys,ExecutionProcessingSys}.cs`（只读参照，不改）。
- 优先语料高频项；每实现一个就在 `dispatch.rs` 的 IMPLEMENTED_* 里登记 + 加单测。

## 硬门禁（自己跑，贴原始输出）
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
python -I tools/flow-diff.py tests/parity/flow-cases.json    # 仍须 3 一致 / 0 不一致
python mir2-rs/tools/... 或 coverage_report 打印未实现清单（给前后对比）
```

## 交付
分小批 patch 挂卡 + `status: needs-review`（每批可独立审查）。**不自行 merge / 不签 review。** 证据四件套写进 body。

## 红线
不得改 `src/**`（oracle）；不得动客户端；一次一批、保持 flow-diff 不回归。
