# 任务：独立复审 M2 主体（Rust GameSvr 世界宿主）——reviewer-2

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`。卡：**`openmir2-m2-gamesvr-host`**，
**被审 head：`6209aa54`**（分支 `worktree-m2-gamesvr-host`；S1 `14a73e4e` 已被它取代）。
作者是 `openmir2-worker-11`，所以你**必须是别的 principal**：本轮用 **`openmir2-reviewer-2`**。

## 0. 先读卡（别只看 diff）

```powershell
cd E:\Users\gxh\Documents\GitHub\OpenMir2
git fetch walgit '+refs/collab/inbox/*:refs/collab/inbox/*'
walgit collab thread openmir2-m2-gamesvr-host
```

卡上有两个 patch（S1 + S2a）与作者的 `self_correction`：**S1 的槽位语义写错了**（oracle 是"复用最小空闲槽"，
作者写成 append-only），S2a 里已自纠。你要重点复核的就是这类"按一段代码推断整体"的地方还有没有。

## 1. 怎么做（判据要自己重跑）

1. 建你自己的 worktree 与身份：
   ```powershell
   git worktree add E:\tmp\openmir2-watch\rev-m2svr 6209aa54
   cd E:\tmp\openmir2-watch\rev-m2svr
   walgit collab join --repo . --principal openmir2-reviewer-2 `
     --key $env:USERPROFILE\.walgit\keys\openmir2-reviewer-2.ed25519 --push walgit
   ```
2. 取判据用**隔离 target**：`$env:CARGO_TARGET_DIR='E:\tmp\openmir2-watch\rev-m2svr-target'`，然后
   `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets --offline -- -D warnings` /
   `cargo test --workspace --offline` / `cargo run -q -p msgcodegen --offline -- --check`，
   game-svr 的用例连跑两遍看 flaky。
3. **与 oracle 源码逐条对**（这是本卡的核心；行号都写在代码注释里）：
   - `src/M2Server/Net/ChannelMessageHandler.cs`：`:434-444`（槽位复用）、`:404`（关闭置 null）、`:161`（按 socket 关）、
     `:146-230`（`GM_*` 分派与 `GM_DATA` 路由优先级）、`:237-289`（`GetCertification`）、`:291-360`（`DoClientCertification`）；
   - `src/GameSrv/Services/AuthenticationService.cs`：`:283-297`（`NewSession`）、`:299-327`（`DelSession`）、
     `:338-368`（`GetAdmission` 的匹配与 `PayMent` 映射）；
   - `src/OpenMir2/HUtil32.cs`：`:359-362`（`StrToInt`）、`:488-546`（`ArrestStringEx` 的两处怪癖与那个越界缺陷）、
     `:650-662`（`TagCount`）、`:12`（`Backslash = '/'`）。
   - **红检**：至少做一次变异（例如把槽位复用改回 append-only、或把 `/` 改成 `\`），确认对应用例会红，然后按字节还原
     （还原后注意 `LESSON_按字节还原源码须重建判断_拷贝保留mtime会让cargo与msbuild跑旧产物`：要确认重编）。
4. 判据里有一条"用真 C# GameGate 实测的 98B 合成登录包"：那份字节在
   `E:\tmp\openmir2-gamegate-imp\oracle-login-cap\out-csharp-raw.json`（真网关在 5010/7210 上跑的产物，
   夹具 `harness.py` 也在同目录，可复跑；**不要占现网 5000/7000/7100/7200**）。

## 2. 签条目

```powershell
walgit collab entry --repo . --kind review --id openmir2-m2-gamesvr-host `
  --parent <该线程最后一条 entry 的 oid> `
  --body '{"agent":"openmir2-reviewer-2","decision":"approve|request_changes","reviewed_commit":"6209aa54", ...}' `
  --push walgit
```

`review` 必须写清：你跑的命令 + 原始结果 + 证据路径；`request_changes` 要给出「哪条判据、什么证据、要求什么」。

## 3. 红线

- **不 merge、不动卡状态**（只签 `review`）；不改作者代码（要复现就 checkout 到自己的 worktree）。
- 无证据不许 approve；拿不到的判据写进 `not_verified` 并说清缺什么。

## 4. 收工

结论一句话 + entry oid 写进 `E:\tmp\openmir2-watch\last-rev-m2svr.md`，并删掉自建 worktree
（`git worktree remove E:\tmp\openmir2-watch\rev-m2svr`）。
