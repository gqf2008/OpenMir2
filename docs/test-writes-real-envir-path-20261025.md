# 测试卫生排查：`Castle/List.txt` 的 mtime 刷新来自**运行中的 C# 服务端**（tier L）

- 卡：`openmir2-test-writes-real-envir-path`（owner `openmir2-dev-2`，branch `card/test-writes-real-envir`）。
- 基线：**排查时** `master = 263e77d7e`（首次观测）；**本次交付** rebase 到 `master = fe14f0603`（行号以交付 head 为准）。
- 写集：本文档（**未改测试、未改代码、未写 `E:/MirServer`**）。
- 结论：**没有任何测试写真实路径**。mtime 刷新来自**仍在运行的 C# oracle 进程**的事实性行为：
  `SystemProcessor.ShowOnline()` 的 **10 分钟**门（`ConsoleShowUserCountTime=600000`）调
  `CastleMgr.Save()`，其中 `SaveCastleList()` 重写 `Castle/List.txt`。

## 1. 写入链（oracle 侧，静态定位）

**被重写的文件（F 订正）**：`E:/MirServer/M2GameSvr/Castle/List.txt`（**不是** `Envir/Castle/List.txt`，
后者目录不存在）。部署路径由 `Server.conf` 两行决定：`:65 CastleDir=Castle`、`:71 CastleFile=Castle/List.txt`
（`M2Share.BasePath` = `E:/MirServer/M2GameSvr`）。周期门配置在 `Server.conf:334 ConsoleShowUserCountTime=600000`。

| 位置 | 内容 |
|---|---|
| `src/GameSrv/Word/Threads/SystemProcessor.cs:54-64` | `ShowOnline()`：`(GetTickCount()-ShowOnlineTick) > Config.ConsoleShowUserCountTime` ⇒ `ShowOnlineTick = now` + `SystemShare.CastleMgr.Save()`；部署 `Server.conf` 的 `ConsoleShowUserCountTime=600000`（10 分钟） |
| `src/M2Server/Castle/CastleManager.cs:146-151` | `Save()`：`SaveCastleList()` + 逐城 `castle.Save()`（`SabukW.txt`） |
| `src/M2Server/Castle/CastleManager.cs:178-190` | `SaveCastleList()`（`:178-191`）：`Path.Combine(BasePath, CastleFile)` → `StringList.SaveToFile` → `StreamWriter(fileName, false, gb2312)`（`src/OpenMir2/Common/StringList.cs:260-270`）逐行写 `0..CastleList.Count-1`（**每个城索引一行**） |

⇒ 该文件是**周期性重写**的：内容不变（部署 2 座城 ⇒ `0\r\n1\r\n` 6B），但 **mtime 每 10 分钟刷新一次**。
这正是本机 `GameSrv`（PID 14040，启动于 2026-10-24 21:41:48）在做的事。

## 2. 观测证据（不写真实文件，只用 `stat` / `sha256sum`）

文件：`E:/MirServer/M2GameSvr/Castle/List.txt`，**6 字节**，
`sha256=9b38d284bae4dc26e593ab0d5cb50d846c3328a8c0467291a8a473cf15c1615b`。

```text
2026-10-25 03:52:02.623   （当时正在跑 castle-official 卡的 6 条用例）
2026-10-25 04:02:02.660   （期间**没有跑任何测试**）
2026-10-25 04:12:02.673   （同上）
```

- 三次读数**每 600s 推进一次**，后两次推进期间**没有任何测试在跑** ⇒ 写者是一个 **10 分钟周期任务**，
  与测试无关。03:52 那次只是「10 分钟 tick 恰好落在跑测试的窗口里」（这就是误判的来源）。
- 复核「重跑不再触碰」：把本卡相关用例（`castle_official` 5 条 + `t40_type2` 1 条）再跑一遍，
  跑前跑后 mtime 都是 `03:52:02`（未变）——变化只由下一个 10 分钟 tick 引起。
- 内容冻结核对：字节 = `30 0d 0a 31 0d 0a`（即 `0\r\n1\r\n`，**6 字节**，无空格；`od -c` 的输出把 `\r`/`\n` 显示成分列，空格是 od 的排版，不是文件内容），与部署两城一致；**本会话从未改其内容**。
- 写者进程：`GameSrv` PID **14040**（起于 2026-10-24 21:41:48），本卡排查期间仍在运行。

## 3. Rust 侧：测试全部注入临时目录（**head sha + grep 命令 + 行号三元组，可机械复核**）

全部在**本卡交付分支 tip** 上执行：`HEAD` = `git rev-parse --short HEAD`（本分支 tip），基线 = `fe14f0603`；
仓库根 = 本 worktree。doc 不写死 sha（避免 amend 自指），交付时的 head 以卡上的 `status` entry 为准。

| 结论 | head | grep 命令（cwd = 仓根） | 实测行号 |
|---|---|---|---|
| `sim_loop` 起世界唯一 `startup_finish` 调用点 | `HEAD` | `grep -n "game_svr::castle::startup_finish(" mir2-rs/bins/game-svr/src/lib.rs` | **20741** |
| 10 分钟门唯一 `save_all(store,…)` 调用点 | `HEAD` | `grep -n "game_svr::castle::save_all(store" mir2-rs/bins/game-svr/src/lib.rs` | **23522** |
| `castle::persist_tests` 的 temp 根/守卫 | `HEAD` | `grep -n "struct TempRoot\|impl Drop for TempRoot\|a22-p3-" mir2-rs/bins/game-svr/src/castle.rs` | **1356**（`mod persist_tests` 声明）、**1362**（模块头注释）、**1375/1377**（注释）、**1379**（`struct TempRoot`）、**1389**（`impl Drop`）、**1401**（路径拼接） |
| `castle::tests` 的 `Fixture` temp 根 | `HEAD` | `grep -n "impl Drop for Fixture" mir2-rs/bins/game-svr/src/castle/tests.rs`；`grep -n "fn fixture(map_name" mir2-rs/bins/game-svr/src/castle/tests.rs`；`grep -n "gs-castle-" mir2-rs/bins/game-svr/src/castle/tests.rs` | **108**（`impl Drop`）、**116**（`fn fixture`）、**118**（`gs-castle-` 根） |
| T137 固定 NPC 夹具 | `HEAD` | `grep -n "impl Drop for T137Rig" mir2-rs/bins/game-svr/src/lib.rs`；`grep -n "gs-t137-" mir2-rs/bins/game-svr/src/lib.rs` | **67353**、**67382**（`T137Rig::new` 在 67360） |
| `mapwire` 真机向量（`#[ignore]` 只读） | `HEAD` | `grep -n "证据/owner 跑" mir2-rs/bins/game-svr/src/mapwire.rs` | **966 / 1060** |
| `npcs` 真机向量（`#[ignore]` 只读） | `HEAD` | `grep -n "MIR2_BASE_DIR" mir2-rs/bins/game-svr/src/npcs.rs` | **905**（注释）、**911**（取值）——文件存在（`wc -l` = 1088） |
| `envir` 侧 merchant 真机向量 | `HEAD` | `grep -n "M2GameSvr" mir2-rs/crates/envir/tests/merchant.rs` | **157**（用法注释）、**166**（默认路径） |

- `--lib` 测试二进制里**没有**任何路径会用真实 `MapLoader` 去 `startup_finish`/`save_all`：两个生产调用点
  （`lib.rs:20741` / `:23522`）只在 `sim_loop` 里，而 `sim_loop` 不被任何单元测试驱动（只被 `mir2-server`
  二进制使用）。
- **口径**：行号会随基线前进漂移（reviewer-21 在旧 head `376289c0e`（base `263e77d7e`）量到
  `959/1049`、`merchant.rs:154`、且认为 `npcs.rs` 不存在；本 head 上这些值不成立）。**以本表三元组
  在当前 head 上复跑为准**；若 reviewer 在其钉住的 head 上复跑结果不同，以其实测改正。

## 4. 处置建议（均不影响本卡判据）

1. **无需修改测试**：本仓 `--lib` 测试不写真实 Envir；禁止项（改 `E:/MirServer` 内容）未发生。
2. **巡检脚本**：把 `Castle/List.txt` 的 10 分钟 mtime 抖动视为**预期**（C# 在跑时）。
   窗口/对拍前记录 mtime，结束后只比对**内容**（`0\r\n1\r\n` 6B），不比 mtime。
3. **若需要「零触碰」**：owner 已授权可停 C# 栈（`GameSrv`/`GameGate`；**别停 mysqld**），
   停后 mtime 不再变。
4. CI 侧已天然隔离（所有落盘用例用 temp 根）。

## 5. 门禁

- 纯文档改动：`conflict_marker` / `stale_pointer` / `dynamic_count_stamp` / `board_coverage` rc=0；
  `git diff --check` rc=0；未跑 cargo（无代码改动）。
- 内容冻结证据：`od -c` = `0 \r \n 1 \r \n`（6B），本会话只读。
