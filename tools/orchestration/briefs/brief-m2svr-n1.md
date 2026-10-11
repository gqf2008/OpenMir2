# 任务：M2 主体卡 openmir2-m2-gamesvr-host —— 收口 reviewer-3 的 N1–N4，然后继续入图出包

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`，HEAD `749ebcea`）。
卡：`openmir2-m2-gamesvr-host`，身份 **openmir2-worker-11**（该 worktree 已 join，命令可省略 `--actor`/`--key`）。
原始派单（门禁/红线/交付口径仍按它）：`E:\tmp\openmir2-watch\brief-m2-gamesvr.md`。

## 0. 先读后写

`git fetch walgit '+refs/collab/*:refs/collab/*'`；`walgit collab thread openmir2-m2-gamesvr-host`。
线程最后一条是 reviewer-3 的 `review`（oid `66c7e33e…`，`reviewed_commit=749ebcea`）：**request_changes**——
N1 `Important(潜在)` + N2/N3/N4 `Minor`。`status` 的 owner 是你自己（worker-11）⇒ 接着干，别重新认领、别新开线程。

## 1. 第一优先：逐条收口 N1–N4（小 diff，快审快合）

- **N1（Important 潜在，必须修）**：`SimToGate::MarkInWorld` 现在发到 `gates.values().next()`（随便挑一条网关线）。
  多网关时会把 `in_world` 标错槽或标不上（各网关 socket 号可重复），后果是后续 `GM_DATA` 被 F3 那条分支门
  当「已认证未进世界」静默丢弃（客户端点了没反应、无报错）。要求：outstanding 存 `(query_id, socket, gate_idx)`
  （`gate_idx` 取自 `info.gate_idx`），`MarkInWorld` 用 `gates.get(&gate_idx)` 路由，与同一处 `KickUser` 完全同构；
  找不到线号时记 WRN，不许静默挑一个。
  **红检（必须做）**：两条假网关 + 故意让 socket 号重复 ⇒ 必须标到正确的那条；把路由改回「随便挑一个」时必须变红。
- **N2（Minor）**：`certify` 落 `Account`/`ChrName` 时 `.trim()`（照 `ChannelMessageHandler.cs:318-319`）；
  `LoadDBInfo` 侧继续用未 trim 的原值（`:325-326`）；把 `users.rs` 里「已 Trim」的失实注释改成事实
  （或按 oracle 口径说清哪里 trim、哪里不 trim）。
- **N3（Minor）**：`ReplyVerdict::TooShort` 不再 `LoadDBErrorCount++`（对齐 `DataQueryServer.cs:199-208`：
  `nLen<12` 的分支什么都不做、也不打日志）。
- **N4（Minor）**：`send_gate_msg` 的 doc comment 里「超过 SocCheckTimeOut(2s) 才清零放行」改成
  「超过 SocCheckTimeOut(50ms) 清零后**仍 Hold**（oracle `:58-66` 无条件 return）」。

收口后在本卡：`comment`（逐条 N1–N4：改了什么 + 证据在哪，含红检的「改坏必红」实测输出）
→ `patch --base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host` → `status: needs-review`。
**先把这一片交出去**（小 diff 便于复审闭环），再进第 2 节。

## 2. 第二优先：继续 S2b-4 —— 入图出包（同 worktree / 同卡，别另开 worker）

按 M2 DoD（真客户端经 Rust 链进图可玩）挑「`SM_*` 入图洪峰的编码与下发」：

- 证据源：M0 金标准 `mir2-rs/tests/golden/session-20261010-c3-hook/capture.jsonl` 的 **329 条 s2c**；
  已实测前 16 条 ident 序列（658 SM_SENDNOTICE → 51 SM_NEWMAP → 50 SM_LOGON → 41 → 213 → 11029 → 42 → 766
  → 54 → 5008 → 52 → 752 → 811 → 46 → 10 …）。**先一条帧逐字节对齐真字节**，再按序判；
  不许只按 C# 源码公式重推（本仓已有教训：常量名/适配器会骗人）。
- 下游发送通道已就绪：`bins/game-gate` 的 `client_frame`。
- 该片与待认领卡 `openmir2-m2-enter-world-frames` 是同一件事 ⇒ 在**那张卡**上认领并交付（同一 principal）；
  不要重复开线程、不要两个 worker 同时写 `bins/game-svr`。

## 3. 门禁（自己重跑，贴原始输出；RED 不许交）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
cargo run -p msgcodegen -- --check
```

- 用**隔离 target**：`CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`（该目录被污染过一次；若报 `NotFound` 假红先怀疑它）。
- master 可能已并入 gamegate（`2bf2f3bd`）或继续前进 ⇒ 把 master 合进分支再取判据；
  已知只会撞 `mir2-rs/Cargo.lock`，配方：`git checkout --ours mir2-rs/Cargo.lock && cargo metadata --offline`
  （证据 `E:\tmp\openmir2-gamegate-imp\mergecheck-gates.txt`）。
- 若 N1–N4 收口片先交，patch 的 `--head` 取你**当时**的分支头，并在 body 里写清 reviewed_commit 与新 head 的差量。

## 4. 红线

- **不起服务栈**：现在 5000/7000/7100/7200 被 rev 车道的 C6 复跑占用（先跑 `E:\MirServer\port-status.ps1` 自查）。
  本卡判据用「假上游 + 进程级」即可，不要抢端口。
- 不改 `src/**`(C# oracle) 行为、不改 `MirClient`、不改表结构/脚本语法/数据文件格式。
- 不 merge 自己的卡、不签 review、不动别人的卡状态。

## 5. 收工回报（给 /root 控制台）

写 `E:\tmp\openmir2-watch\last-m2svr.md`：改了什么、门禁原始结果、patch/status 条目 oid、卡在哪、剩余物清单。
