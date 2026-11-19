# openmir2-rust-build-rev-embedding

> 状态：**已实现（2026-11-19，master `cbe8420f9` 起）**，五颗验收牙均已给出失败原文（见文末
> 「实现与验收」）。本仓库 remote 无 `refs/collab/*`，walgit 看板不可用；按 AGENTS.md §7
> 先落 `docs/issues/`。看板恢复后迁移为正式卡。
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

## 实现与验收（2026-11-19）

### 实现
- `mir2-rs/crates/build-meta`（build-dependency）：`emit()` 取 `git rev-parse HEAD`
  （只收完整 40 hex，否则 `unknown`）、`git status --porcelain` 非空行数、UTC RFC3339 构建时刻；
  重跑触发 = `.git/HEAD` + `index` + **包根**（`.`）；无 git 元数据三者均写 `unknown`，不写空串。
- 8 个交付 bin 各加 `build.rs` + `[build-dependencies]`；`main` 处理 `--version`/`-V`，输出
  `rev=<40hex> dirty=<n> built_at=<ts>`；`--version` 分支带能力标记 `MIR2_BUILD_SELF_REV=1`。
- `stamp-binaries.ps1 -Check`：Rust 件且含能力标记 ⇒ 运行 `<exe> --version`（10s 超时）并断言
  `rev == 侧车 head`、rev 为 40-hex、`dirty == 0`；跑不起来 ⇒ `SELF_REPORT_UNAVAILABLE`；
  无标记的老件/夹具跳过（不是"绿"）。自测用例 11 → 12。
- `legacy_bins_are_thin_wrappers_around_lib_run` 守卫同步：薄包装之外**允许但要求** `--version`
  门存在（删门也红）。
- 第 3 条的 `w-tick-swap.ps1` 在仓库外（`E:/UserFleet`），本次**未改**；它必须"连戳一起拷"或
  只写 `head_source=copied-from-build`。**机制保障已落在本仓闸门**：无论戳是谁写的，只要与
  二进制自报 rev 不符就红（牙 4）。第 4 条的 `start-m1-stack.ps1` 同理在仓库外，建议在起栈前
  调用 `stamp-binaries.ps1 -Check -RequireAll -RequireHead <本轮 head>`。

### 五颗牙（原文）
1. **干净树**：`rev=cbe8420f9bf20d1c4e08cd1e16bf414c9bc30902 dirty=0 built_at=2026-11-19T02:30:36Z`
   （== `git rev-parse HEAD`）；`-Check -RequireHead cbe8420f9` → `STAMP_OK` / `GREEN 1 stamps verified`。
2. **改一字节再构建**：`dirty=1`；`-Check` → `RED … SELF_DIRTY self=1`（rc=1）；还原并重建后
   `dirty=0` 且 `-Check` 回 GREEN。
3. **换提交构建**：在临时 worktree 于 `0be7f0e02` 构建 → `rev=0be7f0e0276c4d24a9b16e938511b768ce234be2`
   （与 `8c66ebf5…` 不同，非常量）。
4. **侧车换"同文件、错 head"**：复制 exe+戳后把戳 `head` 改成存在的 `f80376b37…` →
   `RED … SELF_REV_MISMATCH stamp=f80376b37b78bc53566cf42e466b1fc3ad020267 self=cbe8420f9bf20d1c4e08cd1e16bf414c9bc30902`
   （rc=1，不是 HEAD_NOT_FOUND）。
5. **同源码、不同目录**：两处均在 `8c66ebf5` 构建 `game-svr.exe`——
   `8b18f871…`（probe 目录）vs `2f59c487…`（主检出），size 均 `9,165,824 B`，自报 rev 相同。
   ⇒ sha 不是跨目录稳定身份，`rev` 是。

### 身份链边界（务必知道）
- 本闸门保证的是「**侧车戳 ↔ 二进制自报**一致」；**不能**阻止有人拿**另一份同 head 的二进制**
  （例如别人在同一提交上构建的合法件）来配这个戳——因为 rev 相同、sha 允许不同（见牙 5）。
- 完整身份链 = 自报 `rev` + 侧车 `sha256 == 实物 sha256` + **冻结清单**（谁、在哪次构建里产出了
  这份 sha）+ 构建时间/目录（`built_at`）。缺任何一环，只能证明"来源提交相同"，不能证明"是这
  一份文件"。

### 实测坑（记住）
- `Copy-Item` 还原源文件会带回**旧 mtime** ⇒ Cargo 认为"没变"、build 脚本不重跑，`dirty` 停在
  旧值（本次实测踩到：还原后仍 `dirty=1`）。测试/复核时还原后必须 `touch` 再构建。

## 关联
- 口径：`artifact-identity-proof`（被测/被部署的二进制必须自报身份并被断言）；`gate-green-must-name-the-set`
- 窗口限制记录：`docs/BOTFLEET-M1-REPORT.md` §50（"Rust 件身份只由侧车戳保证"）
