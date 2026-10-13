# M3⑧-3 · reviewer-5 M-1 收口：`load_guild_info` 名单回写失败按 oracle 吞掉

- 卡：`openmir2-m3-guild-file-store`（M3⑧-实现 3）
- 分支：`worktree-m3-guild-store-swallow`
- **base**：`origin/master` = `5964479f3`（`Merge 2b099c46af (worktree-m3-castle-door-repair-o1)`）
- 写集：`mir2-rs/crates/data/src/guild.rs` + `mir2-rs/crates/data/tests/guild_store_vectors.rs` + 本证据目录。
  零 `bins/`、零 `src/ sql/ MirClient`、零 `crates/protocol`、零 `crates/storage`。
- 来源：reviewer-5 在 ⑧-3 第①片（`973aea15a`）approve review（ts 1791880566）的 **M-1（Minor，unregistered-divergence）**，
  其建议为「在接线片显式登记或改成与 oracle 同构的吞错」——本片按 **① 行为等价优先**改成同构吞错（登记留作次选）。

## 1. M-1 是什么（Rust 移植偏差，不是 oracle 行为）

oracle `GuildManager.SaveGuildList`（`src/M2Server/Guild/GuildManager.cs:124-143`）把
`saveList.SaveToFile(SystemShare.Config.GuildFile)` 包在 `try { ... } catch { LogService.Error(...); }`
里（`:135-142`）⇒ **写失败被吞掉**。加载路径 `GuildManager.LoadGuildInfo`（`:78-110`）在
`GuildList.RemoveAt(i); SaveGuildList();`（`:100-101`）之后**继续**循环，不受写失败影响。

Rust 旧实现（⑧-3 第①片，`origin/master:mir2-rs/crates/data/src/guild.rs`）：

```rust
Err(GuildStoreError::MissingGuildFile(_)) => {
    removed.push(name);
    names.remove(i);
    self.save_guild_list(&names, server_index)?;   // 用 ? 把 IO 错误传出 load_guild_info
}
```

`?` 会让 `load_guild_info` **整体返回错误**（中止加载并丢弃已加载结果），而 oracle 只是记一条日志后继续
⇒ 可观测分歧（「加载与回写之间文件变不可写」的竞态下：Rust 中止 / oracle 继续）。本层无 logger，
故按同构语义**仅忽略返回值**，不中止加载。

## 2. 改法与不变量

```rust
let _ = self.save_guild_list(&names, server_index);   // oracle SaveGuildList try/catch 吞错
```

- 移除失败行会后的名单回写失败**不再**使 `load_guild_info` 返回 `Err`；加载继续，`names/removed`
  照常反映被移除的行会（名单文件保持原内容——写失败自然不落盘）。
- 非 `MissingGuildFile` 的**读**错误仍走 `Err(e) => return Err(e)`（未改，oracle 侧 `LoadGuild` 抛错不被本 try/catch 覆盖）。
- 无新增/修改 oracle、表、脚本、协议、客户端；未动 `save_guild_list` 的公开签名或返回值。

## 3. 判据与读数

- **回归向量**（`01`）：新增 `vector_load_guild_info_swallows_list_rewrite_io_error` —— 名单含一个缺
  `<名>.txt` 的会；名单文件**读成功后置只读**（`std::fs::write` 失败）⇒ 断言 `load_guild_info` 仍 `Ok`、
  `removed == ["幽灵会"]`、`names == ["甲会"]`、名单文件保持原内容。`cargo test -p mir2-data --test
  guild_store_vectors` ⇒ **13 passed / 0 failed**。
- **改坏必红**（`02`）：把 `let _ = ...` 改回 `...?` ⇒ 该用例 **FAILED rc=101**，实测
  `Err(Io("拒绝访问。 (os error 5)"))`（即旧行为会中止加载）；`git hash-object` 还原后源 sha256 =
  `ddb1b2463a634d0121748bb4cb3851ec673f799b` 与改前一致，重跑 GREEN。
- **四门禁**（在 base `5964479f3` 的交付树上，`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`，`--offline`）：
  `03` fmt rc=0 / `04` clippy `-D warnings` rc=0 / `05` **101 suites 1191 passed 0 failed 22 ignored** /
  `06` msgcodegen **604** rc=0。
- **边界**（`07`）：仅上述 2 个 `.rs` + 本目录；冻结面 `src/ sql/ MirClient crates/protocol crates/storage bins/` **空**。
- **证据包自证**：`python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <本目录>`
  ⇒ `GREEN [repo 口径]` rc=0；`... eolcheck <本目录>` ⇒ `EOLCHECK GREEN` rc=0（LF 归一 / git blob 口径）。

## 4. 未做（如实登记）

- 本片**只**收口 reviewer-5 M-1，未扩大 ⑧-3 其它面。⑧-3 卡级剩余 DoD 不变：
  `bins/game-svr` 启动 `LoadGuildInfo` / 退出 `SaveGuildList` 接线（归 ⑧-4；等 owner 对 `bins` 串行序裁决）
  + 真机两侧 `run_guild_castle_pair.ps1` diff=0（归 ⑧-10 夹具 + 独占窗口）。
- 未把该偏离登记进 `tests/parity/whitelist.md`：本片选择修到与 oracle 同构 ⇒ 无需登记（登记仅为次选）。

## 5. 残留物清单

- 本目录（证据件 10 件）为本片专属，保留。
- 临时物：`E:/tmp/m3cs-work/guild.rs.fixed`、`guild.rs.fixed2`（红检用副本）与四个门禁日志，均在检出外，非本片专属。
- 隔离编译产物在 `E:/tmp/openmir2-m3cs-target/`（车道共享构建目录）。
