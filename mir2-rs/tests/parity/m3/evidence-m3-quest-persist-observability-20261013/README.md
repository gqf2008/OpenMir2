# evidence-m3-quest-persist-observability-20261013

卡 `openmir2-m3-quest-persist-observability`（M3⑥-实现 4：固化 oracle「任务态不落库」语义 + 门禁）证据包。

全部命令在**本 worktree**（`E:/Users/gxh/Documents/GitHub/OpenMir2/.claude/worktrees/m3pt`）内、
`CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`（检出外）。**未起栈、未占端口**。

## 交付物

| 文件 | 内容 |
| --- | --- |
| `mir2-rs/bins/db-srv/tests/quest_no_persist.rs` | 门禁本体：四层断言（源码扫描 / SQL 正面+反面 / 会话态仍在 / 已捕获两侧基线 0 行）+ 自动红检 |
| `mir2-rs/bins/db-srv/Cargo.toml` | 仅新增 `[dev-dependencies] serde_json = "1"`（读 M1 跨读基线） |
| 本目录 | 证据四件套 |

## 证据文件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 复现命令清单 |
| `01-oracle-anchors.txt` | `src/` 实测行号（`Save.cs:80-87/493`、`Query.cs:192`、`PlayerDataInfo.cs:101-116`）+ `sql/mir2_db.sql:371936` + `SaveQuest` 无调用点 |
| `02-rust-status-greps.txt` | Rust 侧现状：`characters_quest` 0 引用 / db-srv 无任务列 / 会话态仍在 / 两侧基线 0 行 |
| `03-gate-quest-no-persist.txt` | 门禁读数：5/5 passed |
| `04-redcheck-break-must-be-red.txt` | 改坏必红 3 条（RC1 任务列 / RC2 `characters_quest` 写路径 / RC3 基线行数），逐条 FAILED + 还原 sha256 一致 |
| `05-gate-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `06-gate-clippy.txt` | `cargo clippy --workspace --all-targets -- -D warnings` rc=0 |
| `07-gate-test.txt` | `cargo test --workspace` rc=0：95 suites / 914 passed / 0 failed / 21 ignored |
| `08-gate-msgcodegen.txt` | `cargo run -p msgcodegen -- --check` rc=0：604 条常量 |
| `09-boundary.txt` | `git diff --name-only <base> <head> -- src sql MirClient` 空 + 全量改动清单 |

## 口径

`SHA256SUMS.txt` 按**仓库字节（Git blob / LF 归一）**记，复核：

```sh
python -I tools/evidence-hash/verify_sha256sums.py verify mir2-rs/tests/parity/m3/evidence-m3-quest-persist-observability-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-quest-persist-observability-20261013
```

## 结论 / 边界

- **oracle**：MySQL 下任务标志只存在于会话内 `CharacterData`（`PlayerDataInfo.cs:101-116`，走 6000 面），
  `SaveQuest`（`Save.cs:493`）只 `DELETE FROM characters_quest`、从不 INSERT、**全仓无调用点**；
  `characters` 的 UPDATE/SELECT 列清单（`Save.cs:81-86` / `Query.cs:192-194`）**无任务列** ⇒ **下线即丢**。
  这是参照实现的行为，不是缺口（盘点报告 `M3⑥任务现状盘点报告-20261013.md` §1.F / §2 #12）。
- **门禁**：把「Rust 侧不写 `characters_quest`、`characters` 无任务列、会话态仍在」钉死；注入任何一条
  任务落库路径即红（RC1/RC2）。若日后要真持久，属**规格变更**，需 owner 签字另立卡。
- **真机半边**（本卡 DoD 的进程级判据：任务交互后两侧 `characters_quest = 0 行` + `characters` 无任务列变更）
  **未跑**：需独占窗口（`run_pair.ps1` + `scoped_diff.py --canonical`，`characters_quest` 已在
  `scoped_diff.py` 的 `TABLES` 作用域内）。本片以 M1 跨读**已捕获的两侧快照**（`03/04-csharp/rust-dbsrv-baseline.json`）
  做等价固定；缺口 = 需在节点后用**任务交互**后快照重跑同一判据 ⇒ 登记为 **not_verified（窗口）**。

## 残留物

- `E:/tmp/openmir2-m3pt-target/`（构建缓存，登记保留）。
- 无临时备份残留：改坏必红的备份 `E:/tmp/_m3pt_playdata.bak`、`E:/tmp/_m3pt_cs_baseline.bak` 已用完即删。
