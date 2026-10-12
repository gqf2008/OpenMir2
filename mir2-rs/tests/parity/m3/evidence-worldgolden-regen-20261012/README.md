# WorldGolden 生成器重跑复算（2026-10-12）

卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）
worktree `.claude/worktrees/m3cs` / branch `worktree-m3-combat-skills`
复算 head：`de0ea5927a263fd052140216d2621c6f1bf65fd1`（双端同步的 master）

## 1. 背景（本片要收的未核项）

攻击路径三差片（`716ec8b8` @ head `1fc46f4c`）在 `src` 侧改动之后，**重基准了 `combat.txt` /
`combat_exp_hitter_ghost.txt`**（WorldGolden 桩 `StubsCombat.cs` 按 oracle 补
`AttackSuccess()` 的 `Random(5)`，见 `M3RNG攻击路径三差归因报告-20261012.md`）。审查侧登记了一条
**未核项**：

> 「C# WorldGolden 生成器未重跑（本片只读桩/golden diff）」

即：审查者核对的是**桩与 golden 的 diff 只覆盖预期路径**，但**没有独立重跑 C# 生成器**，
因此无法从「生成器输出」这一侧证明仓库内 golden 就是当前桩的产物。这正是 F-1
（`LESSON_崩溃复现证据必须当场归档快照再重跑否则被后续运行覆盖`）同族的**证据可复算性**问题。
本片只做这一件事：**用当前桩重跑生成器，与仓库内 golden 逐字节比对**。

## 2. 结论（可复算）

- **5/5 golden 逐字节相同**（`basic / session / combat / combat_exp_nofix / combat_exp_hitter_ghost`）；
- `combat_exp_nofix.txt` **未被本片改动过**，重跑仍逐字节相同 ⇒ 改动范围只落在声明的两路
  （`combat.txt` / `combat_exp_hitter_ghost.txt`），没有溢出；
- 对应判据 `cargo test -p mir2-parity-tests --test world_combat_parity` = **7 passed; 0 failed**
  （Rust 世界模拟 vs 仓库内 golden，即重产的同字节件）；
- 红检：把重产件改一个字节 ⇒ `cmp` 必 `DIFFER`（比对非平凡，不是空真）。

⇒ 审查侧的未核项 **③「C# WorldGolden 生成器未重跑」在本片收口**：
仓库内 golden = 当前桩的生成器输出（版本钉住见 `05-git-and-source-pins.txt`）。

## 3. 复算产物（路径 + hash + 字节数）

| scenario | bytes | sha256（盘上原始字节 = 仓库字节，`.gitattributes: text=set`） |
| --- | --- | --- |
| basic | 5953 | `b6c22bf858680ff6abea64adc5dfe19f3903b8b7f929c2027dc0cfcda51565f8` |
| session | 861 | `048eb5312e16daa83c445ec6432c1eb13020a9db69a9cc8c92db8486996437ca` |
| combat | 6056 | `dd5bc8b33c975f8634f6b05d1916dfe2298e72031b9ac260969a22c18502cc5e` |
| combat_exp_nofix | 303 | `5317bd54e7b1ed2323ba94dbe568cb38c9571c2eac9fb6e57de0ad90e412f092` |
| combat_exp_hitter_ghost | 521 | `669aa341caffc0034030add441b18b93222e783a876bf21912050216edb3b221` |

重产产物落**仓外临时目录** `E:/tmp/m3cs-wg-regen-evidence/`（重产文件与仓库内 golden 同 hash），
**未写回仓库**；仓库内 golden 在本片**零改动**（`git status` 只见本证据目录）。

## 4. 原始件清单

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 全部命令（可直接重放） |
| `01-dotnet-build-fresh.txt` | `dotnet build --no-incremental` 输出（0 warning / 0 error） |
| `02-regen-and-cmp.txt` | 5 份重产 + `cmp` 逐字节结果（5/5 IDENTICAL） |
| `02a-redcheck-tamper.txt` | 改坏必红（篡改 1 字节 ⇒ DIFFER） |
| `03-sha256-regen-vs-committed.txt` | 重产件 vs 仓库内 golden 的 sha256/字节数 + git blob 号 |
| `04-cargo-combat-parity.txt` | `world_combat_parity` 7 passed; 0 failed |
| `05-git-and-source-pins.txt` | head/master/桩/场景的 blob 号、`.gitattributes` text 属性 |
| `06-gates-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `07-gates-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` rc=0 |
| `08-gates-test.txt` | `cargo test --workspace --offline` 83 suites / 674 passed / 0 failed / 16 ignored / rc=0 |
| `09-gates-msgcodegen.txt` | `msgcodegen --check` 604 条与 C# 源一致 / rc=0 |
| `README.md` | 本文件 |

## 5. 复现

见 `00-commands.txt`（一条条重放）。要点：
`dotnet build --no-incremental`（排除陈旧 bin/obj）⇒ 用生成的 dll 重产到仓外目录 ⇒
`cmp` 仓库内 golden ⇒ `cargo test -p mir2-parity-tests --test world_combat_parity --offline`
（`CARGO_TARGET_DIR` 隔离）。

## 6. 边界与纪律

- **零 `.rs`**、**零 golden 改动**、零判据/零门改动；只新增本证据目录。
- 未动 `src/**` / `sql/` / `MirClient` / `bins/**`。
- 未起栈、未占端口（本片全离线；端口状态与开工前一致，仅共享 3306）。
- 未 merge、未自签 review。

## 7. 残留 / 未核项（如实登记）

- **DoD② 真机两相逐 k=0 仍未达成**：硬前置 = `bins/game-svr` 把「前方格无目标」支接到
  `World::attack_swing`（owner 已派 m2-svr，`brief-m2svr-next.md` 顶部 20:10 块）。接线合入后
  才取 `0137` 窗口跑 `20 冒烟 → 120 两相 → attack_diff align/diff`。
- **残留物清单**：仓外临时目录 `E:/tmp/m3cs-wg-regen-evidence/`（重产 golden 5 件，供复核；可删）、
  `E:/tmp/m3cs-golden-regen/`、`E:/tmp/m3cs-golden-regen2/`（早期两次同口径重产，可删）、
  `E:/tmp/m3cs-golden-verify-target/`（隔离 cargo target）。
  以上均**在仓库之外**，仓库内无本片专属残留。
