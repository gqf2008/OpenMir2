# 任务：独立审查（本轮身份 openmir2-reviewer-4）——**只审这两张卡的指定 commit**

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`。你已在隔离检出 `E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2rev`（detached HEAD = `d1a7bfd5`）。
身份：`openmir2-reviewer-4`（**不得与作者同 principal**；本卡作者是 `openmir2-worker-11`）。
先 `walgit collab join --repo . --principal openmir2-reviewer-4 --key C:\Users\gxh\.walgit\keys\openmir2-reviewer-4.ed25519 --push walgit`。

## 审什么（**别碰别的待审卡**——rev 车道正在按队列走，撞同一评审面只会产生重复 review）

1. **`openmir2-m2-gamesvr-host` @ `d1a7bfd5`**（作者 worker-11；卡内 patch 条目 + `status` 的 `reviewed_commit=749ebcea`）：
   - 内容：收口 `openmir2-reviewer-3` 的 `request_changes`（oid `66c7e33e`）N1–N4。
   - **必须重跑**（不采信作者自述）：
     - 四门禁（**隔离 target**：`CARGO_TARGET_DIR=E:\tmp\openmir2-rev4-target`）：fmt / clippy `--workspace --all-targets --offline -D warnings` /
       `cargo test --workspace --offline` / `cargo run -p msgcodegen -- --check`；
     - **三条红检自己复现**：① `route_to_gate` 改回 `gates.values().next()` ⇒ `mark_in_world` 两条用例必须红；
       ② `certify` 去掉 `.trim()` ⇒ `certify_trims_account_and_chr_name` 必须红；③ `TooShort` 改回 `+=1` ⇒ 对应用例必须红。还原后必须全绿。
     - 读 N1 的实现：`outstanding` 是否带上 `info.gate_idx`、`MarkInWorld` 与 `KickUser` 是否同构走 `gates.get(&gate_idx)`、
       线号缺失是否记 WRN 而不是静默挑一条；确认 N2 的 trim 只作用于 `gateUser` 落字段而 `LoadDBInfo/LoadDbRequest` 仍用未 trim 原值（对 `ChannelMessageHandler.cs:318-319` vs `:325-326`）。
   - 结论写进 `review` 条目：逐条给 N1–N4 的「已收口/未收口 + 依据」，再加你自己发现的问题（含 severity）。
2. **`openmir2-m2-enter-world-frames` @ `dc33c1ca`**（同作者；增量 `d1a7bfd5..dc33c1ca`，只 `smframe.rs` + `main.rs` 一行 `mod`）：
   - 判据：7 个 `smframe` 单测 + 2 条红检（Param/Tag 序反 ⇒ 4 红；`client_frame` 不 Encode 头 ⇒ 5 红）自己复现；
     并**独立验语义**：`#`+Encode(头12B)+尾原样+`!` 是否与 oracle `ClientSession.cs:897-920` 一致、
     decode 出的 12B CommandMessage 字段序是否对；owner 侧旁证见该卡 comment `ae7e2e76`（5 条常量与 ident 配对已用另一条路径核过，**不含**编码语义）。
   - 顺序判据②（入图序到 `SM_TURN` 同序且长度相等）作者已登记未做 ⇒ 在 review 里记明「不覆盖」。

## 边界

- 只改证据/审查相关，**不动被审代码**（要改正就在隔离 target 里改后还原，并把「改坏必红 → 还原必绿」的原始输出留档到 `E:\tmp\openmir2-watch\rev4-ev\`）。
- **不 merge、不改卡状态**（status 由作者/owner 移）；`review` 的 parent 用该线程最后一条 entry 的 oid。
- 收工写 `E:\tmp\openmir2-watch\last-rev4.md`：审了哪两张、结论、实跑命令与证据路径、跳过与原因。
