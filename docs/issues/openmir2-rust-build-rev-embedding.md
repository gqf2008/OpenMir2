# openmir2-rust-build-rev-embedding

> 状态：**已登记，未建卡**（本仓库 remote 无 `refs/collab/*`，walgit 看板不可用；按 AGENTS.md §7
> 「tracker 未就绪时先落 `docs/issues/`」过渡）。看板恢复后迁移为正式卡。
> 发现于 2026-11-19 `RUN-START tick-telemetry` 窗口准备（证据链缺口，opc-07 立卡）。

## 现象
Rust 侧交付件（`game-svr` / `game-gate` / `login-gate` / `sel-gate` / `mir2-server` / `db-srv` /
`login-srv` / `res-svr`）**不内嵌 revision**：`strings`/字节扫描都找不到 40-hex 的 HEAD。

- C# 侧有 `check_stamp_head.py` 读程序集里的 `AssemblyInformationalVersion`，能做「二进制自报 == 期望 head」；
- Rust 侧没有任何等价物 ⇒ 身份**只由侧车 `.build.json` / `.stamp.json` 保证**，而侧车是"旁边的一个文件"，
  与二进制之间没有密码学绑定（sha256 只证明"戳和文件一致"，不证明"戳是构建时写下的"）。

## 风险（已实际出现过两种）
1. **换件脚本自行铸戳**：`E:\UserFleet\w-tick-swap.ps1` 的 `Write-Stamp` 按"我从哪个目录拷的"写 `head`，
   `head_source=explicit`。若拷错件（或分支 tip 与构建 head 不同），戳会**诚实地写错**，而闸门照绿。
2. **两套戳命名并存**：新工具写 `X.build.json`，旧脚本写 `X.stamp.json`；同一目录两种都在时，
   校验器读到哪一份取决于实现细节，容易被"旧戳还在"蒙混过关。

## 建议改动
1. `build.rs` 用 `git rev-parse HEAD` + `git status --porcelain` 产出
   `MIR2_BUILD_REV` / `MIR2_BUILD_DIRTY`（脏项数）/ `MIR2_BUILD_AT`，以 `option_env!` 编进二进制；
   无 git 元数据时编入 `unknown`（**不得**静默写成空串）。
2. 每个 bin 提供 `--version`（或启动首行 `INF`），一行机读：`rev=<40hex> dirty=<n> built_at=<ts>`。
3. 侧车保留为旁证，但**命名统一为 `X.build.json`**；`w-tick-swap.ps1` 改为**拷构建产物时连戳一起拷**，
   删掉自带铸戳逻辑（或只允许 `head_source=copied-from-build`）。
4. 闸门（`start-m1-stack.ps1` 与 `stamp-binaries.ps1`）对 Rust 件增加一条：**二进制自报 rev == 期望 head**。

## 验收（四颗牙，各自须有失败原文）
1. 干净树干构建 ⇒ `--version` 的 `rev` 与 `git rev-parse HEAD` 逐字相等、`dirty=0`；
2. 改一个字节再构建 ⇒ `dirty≥1`（且闸门能因此变红）；
3. 换一个提交构建 ⇒ `rev` 随之改变（不是常量）；
4. **部署后取证**：`m1-bin` 里的件自报 rev == 本轮期望 head，且侧车 sha256 == 实物 sha256；
   故意把侧车换成"同文件、错 head"⇒ 闸门红。

## 实证（2026-11-19）：构建路径也会改变 sha

同一份源码、同一工具链（`rustc 1.98.1` / `cargo 1.99.0`）、**大小完全相同**，但 sha 不同：
- `aoi-phase2-impl` 车道 @ `717093150` → `game-svr.exe` = `9d4e4f16…`（9,164,288 B）
- 主检出 @ `c3d3c271`（=M；`git diff 717093150 c3d3c271 -- mir2-rs` 为空）→ `37826a1b…`（9,164,288 B）
- `ctrl-m1` 工作区 @ `c3d3c271` → `42171743…`（9,164,288 B，opc-07 报告值）

⇒ **sha 不是跨机器/跨目录的稳定身份**，`rev` 才是。这抬高了"侧车戳 + 自报 rev"的必要性。

追加验收牙（第 5 颗）：**同源码、不同目录构建 ⇒ sha 不同，但 `--version`/首行 `INF` 自报的
`rev` 相同**——正好证明自报 rev 的必要性；反例牙：把 rev 改成常量或取自构建目录名 ⇒ 本牙红。

## 关联
- 口径：`artifact-identity-proof`（被测/被部署的二进制必须自报身份并被断言）；`gate-green-must-name-the-set`
- 窗口限制记录：`docs/BOTFLEET-M1-REPORT.md` §50（"Rust 件身份只由侧车戳保证"）
