# evidence-m3-guild-file-store-f2-20261013 — ⑧-3 **重开 f1**（基于当前 master）

- 卡：`openmir2-m3-guild-file-store`（M3⑧-实现 3 行会文件加载/落盘）
- 车道：`openmir2-worker-5` / worktree `m3cs` / branch `worktree-m3-guild-file-store-f2`
- base：`origin/master 0f9fe9997a4ed717ee1361bfb2134329fd04472a`
- head：`144ee7b4e06f5f617f960fda19100b3291f80669`
- 触发：合并检查点车道 comment `fd66c1c6`（`git merge-tree origin/master 78dd46d19` ⇒ CONFLICT
  modify/delete ×6：`guild.rs` / `guild_store_vectors.rs` / 报告 / probe 3 件——原片 `f2361cf2` 曾被
  并入 master `dcbd60f3d`、随后按 reviewer-6 暂缓请求 revert `80be29207`，f1 基线仍是 `dcbd60f3d`）。

## 本片做了什么

**只做重开**，不新增行为：

1. 新分支 `worktree-m3-guild-file-store-f2` 从当前 `origin/master 0f9fe9997` 起（**非破坏性**：不改写
   已推的 f1 历史，未 force-push）；`git merge f2361cf2` 对当前 master 是 **no-op**（`f2361cf2` 已是
   master 祖先、内容被 revert 掉），故直接以 F-1 head `78dd46d19` 的最终文件覆盖本卡文件。
2. `crates/data/src/lib.rs` **手工并集**：保留 master 侧新增模块（`pet_slave` 等），追加 `pub mod guild;`
   与本卡 doc 的「唯一写盘面」例外段。
3. `git diff 78dd46d19 -- <本卡文件/目录>` = **空**（内容与 F-1 head 逐字节一致，见 `01-reopen-identity.txt`）。
4. `git merge-tree --write-tree origin/master HEAD` = **rc=0、无 CONFLICT**（合并车道要求的先决）。

## 判据与读数

| 判据 | 命令 | 读数 | 原始件 |
| --- | --- | --- | --- |
| 内容同一性 | `git diff 78dd46d19 -- <本卡文件>` | **空**（逐字节 == F-1 head） | `01-reopen-identity.txt` |
| 可干净合并 | `git merge-tree --write-tree origin/master HEAD` | **rc=0**、无 CONFLICT 段 | `01-reopen-identity.txt` |
| 边界 | `git diff --name-only origin/master HEAD -- src sql MirClient` | **空** | `02-boundary.txt` |
| 边界（bins） | `git diff --name-only origin/master HEAD -- mir2-rs/bins` | **空**（未动 bins/game-svr） | `02-boundary.txt` |
| Rust 向量用例 | `cargo test -p mir2-data` | `guild_store_vectors` **12/12** | `08-gate-test.txt` |
| **改坏必红** | 还原 F-1 原实现（`known` 初始名单快照） | `vector_load_guild_info_uses_alive_set` **FAILED**（`left: 2 / right: 1`，点名 F-1）rc=101；还原后 12/12 绿 | `03-redcheck-f1-alive-set.txt` |
| 四门禁 | fmt / clippy / workspace test / msgcodegen | fmt0 / clippy0 / **96 suites 982 passed 0 failed 21 ignored** / msgcodegen **604** | `06..09-gate-*.txt` |

> F-1 的行为语义、oracle 锚（`GuildManager.cs:67-77/94-103` 存活集）与真身探针读数见未改动的
> `evidence-m3-guild-file-store-f1-20261013/`（同分支同一批文件，逐字节一致）。

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 命令四件套 |
| `01-reopen-identity.txt` | 内容同一性 + merge-tree + 对 master 的新增清单 |
| `02-boundary.txt` | 边界（src/sql/MirClient/bins 空） |
| `03-redcheck-f1-alive-set.txt` | 改坏必红（还原 F-1 原实现 ⇒ 红；还原后绿） |
| `06..09-gate-*.txt` | 四门禁原始输出（base = 当前 master） |
| `SHA256SUMS.txt` | 仓库 LF/git-blob 口径校验清单 |

## 残留 / 承接（不变）

- `bins/game-svr` 启动 `LoadGuildInfo` / 退出 `SaveGuildList` 接线 ⇒ ⑧-4 `openmir2-m3-guild-state`。
- 真机两侧 `run_guild_castle_pair.ps1` diff=0 ⇒ ⑧-10 夹具 + 独占窗口（本片零起栈、零端口）。
- 本片**不 merge、不自签 review**。
