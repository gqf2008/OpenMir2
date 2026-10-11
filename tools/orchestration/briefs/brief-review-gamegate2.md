# 任务：独立复审 M2 GameGate 的新 head（不是 612222f8）——reviewer-3

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`。你要审的卡：**`openmir2-m2-gamegate`**
（Rust `bins/game-gate`），**被审 commit：`f46f8b45`**（分支 `worktree-m2-gamegate`）。
作者是 `openmir2-worker-10`，所以你**必须是别的 principal**：本轮用 **`openmir2-reviewer-3`**。

## 0. 为什么有这一轮复审（先读，别重复别人的弯路）

- `612222f8` 曾被 reviewer-4 `approve`，但那份 approve 是按**作者给的假设**验的（公式对照 + `Split('\\', …)` 语义），
  **没有任何一条判据碰过真客户端字节**。作者随后用真 C# 网关（副本跑 5010/7210）+ 真客户端的金标准帧实测，
  发现三处口径错误，其中两处会让**真客户端全部被剔除**（入站 20B 头、登录分隔符 `\` vs `/`）。
- 所以本轮要审的是**新 head `f46f8b45`**，重点是"这三处改对了吗、有没有把别的东西改坏"。
- 全部实测证据与夹具：`E:\tmp\openmir2-gamegate-imp\oracle-login-cap\`
  （`FINDINGS.md` 是结论表；`harness.py` 可复跑；`out-csharp-raw.json` 是真网关的输出）。

## 1. 怎么做（判据要**自己重跑**，不采信作者自述）

1. 建你自己的 worktree 并加入身份：
   ```powershell
   cd E:\Users\gxh\Documents\GitHub\OpenMir2
   git worktree add E:\tmp\openmir2-watch\rev-gg3 f46f8b45   # detached 即可，别改作者分支
   cd E:\tmp\openmir2-watch\rev-gg3
   walgit collab join --repo . --principal openmir2-reviewer-3 `
     --key $env:USERPROFILE\.walgit\keys\openmir2-reviewer-3.ed25519 --push walgit
   ```
2. 取判据（**用隔离 target**，别用别人 worktree 的 target）：设 `CARGO_TARGET_DIR=E:\tmp\openmir2-watch\rev-gg3-target`，然后
   `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets --offline -- -D warnings` /
   `cargo test --workspace --offline` / `cargo run -q -p msgcodegen --offline -- --check`，
   并把 game-gate 的用例跑**两遍**看有没有 flaky。
3. **重点独立判据（本批的四条）**：
   - `oracle_golden_login_frame_yields_oracle_synthetic_packet`：真客户端帧（111B）⇒ 输出必须逐字节等于真 C# 网关
     吐的 98B。**你自己另跑一次真网关**（`python E:\tmp\openmir2-gamegate-imp\oracle-login-cap\harness.py csharp raw`），
     把 `out-csharp-raw.json` 里那条 `GM_DATA` 的 `body_hex` 与用例里的常量比对——**不许只看用例绿**。
   - `header_prefixed_login_is_kicked`：framing 红检；再自己变异一次（把读循环改回"读 20B 头"）确认它会红。
   - `downlink_frames_match_oracle_bytes`：期望值应是真网关实测字节（`PackLength=+16` / `-4` 两种形态）。
   - `oversize_frame_is_dropped_without_kick`：超 `NomClientPacketSize=400` 只丢不关（oracle `ClientSession.cs:196-199`）。
4. 顺手核对（diff 级）：`login.rs` 的 `FIELD_DIVIDER='/'` 是否与 `src/OpenMir2/HUtil32.cs:12`
   （`public const char Backslash = '/';`）一致；`main.rs` 的 `client_frame()` 是否与
   `ClientSession.cs:891-924` 一致；文档 §2 的更正与 §7.12 是否与实测一致。

## 2. 签条目（结论只能是你自己跑出来的）

```powershell
walgit collab entry --repo . --kind review --id openmir2-m2-gamegate `
  --parent <该线程最后一条 entry 的 oid：先 `walgit collab thread openmir2-m2-gamegate` 拿> `
  --body '{"agent":"openmir2-reviewer-3","decision":"approve|request_changes","reviewed_commit":"f46f8b45",
           "verified_ok":[...你跑的命令+结果+证据文件路径...],"findings":[{"severity":"important|minor","where":"...","problem":"...","repro":"...","suggestion":"..."}]}' `
  --push walgit
```

## 3. 边界（红线）

- **不要 merge、不要移动卡状态**（status 由 owner/coordinator 动）；只签 `review`。
- 不要改作者的代码/分支；要复现就 checkout 到**你自己的** worktree。
- 不要用现网 5000/7000/7100/7200（C6 压测正在用）——本批复现只需 `harness.py` 的 5010/7210。
- 拿不到证据就写 `request_changes` 并说清缺哪条，不许"无证据 approve"。

## 4. 收工

把你的结论（决定性的一句话）、签的 entry oid、跑过的命令与证据路径，写进
`E:\tmp\openmir2-watch\last-rev-gg3.md`，并删掉你自己建的 worktree（`git worktree remove E:\tmp\openmir2-watch\rev-gg3`）。
