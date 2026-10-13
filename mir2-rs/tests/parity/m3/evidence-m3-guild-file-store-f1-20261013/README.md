# evidence-m3-guild-file-store-f1-20261013 — ⑧-3 reviewer-6 **F-1** 返工证据包

- 卡：`openmir2-m3-guild-file-store`（M3⑧-实现 3 行会文件加载/落盘）
- 车道：`openmir2-worker-5` / worktree `m3cs` / branch `worktree-m3-guild-file-store-f1`
- base：`origin/master dcbd60f3d`（原片 head `f2361cf2` 已被合并车道并入 master `dcbd60f3d`）
- head：`d0f90e4012344ca63e87607355cfb8c55a657f0e`
- 发现来源：`openmir2-reviewer-6` 在 head `f2361cf2` 上的独立复核（`kind=comment`，
  覆盖于 reviewer-5 approve `d32263d3` 之后）——**F-1（Important）**。

## F-1 是什么

`GuildStore::load_guild_info`（`crates/data/src/guild.rs`）在**逆序**加载时，用**回写失败前**的
`known: BTreeSet<String>` 名单快照回答 `guild_exists`。而 oracle：

- `GuildManager.LoadGuildInfo`（`GuildManager.cs:78-110`）对每个加载失败的会**当场**
  `GuildList.RemoveAt(i)` + `SaveGuildList()`（`:94-103`）；
- `GuildInfo.LoadGuildFile` 的敌对段（case 2）用 `SystemShare.GuildMgr.FindGuild(name)`
  （`GuildInfo.cs:280-289`），而 `FindGuild`（`GuildManager.cs:67-77`）遍历的是**当前** `GuildList`。

⇒ 当低索引会 A 的敌对段引用了**高索引、已在本轮被移除**的会 B 时：oracle `FindGuild("B")==null`
⇒ **不入表**；Rust 因 `known` 仍含 B 而**误入表**。

## 修法

`load_guild_info` 删除 `known` 快照，闭包改为查**当前 `names`**（逆序中失败者已
`names.remove(i)`，成功者与尚未处理者仍在 ⇒ 恰为 oracle 的存活集）。oracle 锚见
`01-oracle-anchors.txt`。

## 判据与读数

| 判据 | 命令 | 读数 | 原始件 |
| --- | --- | --- | --- |
| oracle 真身向量 | `dotnet run --project mir2-rs/tests/parity/m3/guild-store-probe -c Release -- E:/tmp/m3cs-gfs-f1b` | `wrote 96 fields / 4 byte vectors`（原 89/3）；重复实跑 `cmp` 无输出 | `02-probe-run.txt` |
| Rust 向量用例 | `cargo test -p mir2-data --offline --no-fail-fast` | **12/12 passed**（新增 `vector_load_guild_info_uses_alive_set`） | `03-rust-vectors.txt` |
| **改坏必红** | 还原 `known` 快照 ⇒ 重跑 | `vector_load_guild_info_uses_alive_set` **FAILED**（`left: 2 / right: 1`，点名 F-1）；还原后 12/12 绿 | `03-redcheck-f1-alive-set.txt` |
| 四门禁 | fmt / clippy / workspace test / msgcodegen | fmt0 / clippy0 / **96 suites 961 passed 0 failed 21 ignored** / msgcodegen **604** | `06..09-gate-*.txt` |
| 边界 | `git diff --name-only dcbd60f3d HEAD -- src sql MirClient` | **空**（只动 `crates/data` + `tests/parity/m3`；未动 `bins/game-svr`） | `10-boundary.txt` |

新增向量（`I_alive_set`，探针 `ScenarioI` 直调真 `GuildManager`）：

```
I_alive_set  find_存活会            True
I_alive_set  find_同盟会            True
I_alive_set  find_已亡会            False
I_alive_set  war_count              1
I_alive_set  war_0_name             同盟会
I_alive_set  war_has_removed        False
I_alive_set  member_已亡会          False
# 逐字节：I_alive_set_list_rewritten 名单回写为 存活会\r\n同盟会\r\n
```

场景：名单 `存活会\n同盟会\n已亡会\n`；`存活会.txt` 敌对段 `+同盟会 100` + `+已亡会 200`；
`已亡会.txt` 缺失 ⇒ 逆序先移除 `已亡会`，随后 `存活会` 的敌对段只保留 `同盟会`（war_count=1）。

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 命令四件套 |
| `01-oracle-anchors.txt` | oracle 行号锚（FindGuild / RemoveAt / case2 / SaveGuildList） |
| `02-probe-run.txt` | C# 真身探针实跑输出 + 新增向量 + 临时树 |
| `03-rust-vectors.txt` | Rust 用例实跑（12/12） |
| `03-redcheck-f1-alive-set.txt` | 改坏必红（还原 F-1 原实现 ⇒ 红；还原后绿） |
| `06-gate-fmt.txt` / `07-gate-clippy.txt` / `08-gate-test.txt` / `09-gate-msgcodegen.txt` | 四门禁原始输出 |
| `10-boundary.txt` | 边界 |
| `11-vectors-inventory.txt` | 向量清单与分组计数 |
| `SHA256SUMS.txt` | 仓库 LF/git-blob 口径校验清单 |

## 残留 / 承接（不变）

- `bins/game-svr` 启动 `LoadGuildInfo` / 退出 `SaveGuildList` 接线 ⇒ ⑧-4 `openmir2-m3-guild-state`。
- 真机两侧 `run_guild_castle_pair.ps1` diff=0 ⇒ ⑧-10 夹具 + 独占窗口（本片零起栈、零端口）。
- 本片**不 merge、不自签 review**。
