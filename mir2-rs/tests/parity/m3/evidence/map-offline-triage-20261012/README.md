# 证据包：M3 DoD③ map-bound 7 行离线三分类 + (a) 类真身向量（2026-10-12）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／principal `openmir2-worker-5`
- worktree：`.claude/worktrees/m3cs`／branch `worktree-m3-combat-skills`／base master `e0f9adad`
- 报告：`mir2-rs/tests/parity/m3/M3技能map-bound离线可行性三分类与真身向量报告-20261012.md`
- 窗口：未占端口、未起栈（全离线）；DoD①② 真机对拍仍未达成（沿用第九片登记）。

## commands（命令 + 原始输出）

| 文件 | 命令 | 结果 |
| --- | --- | --- |
| `01-probe-build.txt` | `dotnet build mir2-rs/tests/parity/m3/map-effect-probe/MapEffectProbe.csproj -c Release` | rc=0，0 warning / 0 error |
| `02-probe-run.txt` | `dotnet run --project mir2-rs/tests/parity/m3/map-effect-probe -c Release --no-build -- .` | 写 160 cases / 7 rows |
| `03-probe-repro.txt` | 连跑两次比对 sha256 | 逐字节相同（`ActorId` 固定） |
| `04-fmt.txt` | `cargo fmt --all -- --check` | rc=0 |
| `05-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `06-test-workspace.txt` | `cargo test --workspace --offline` | rc=0；80 suites / 663 passed / 0 failed / 16 ignored |
| `07-msgcodegen.txt` | `cargo run -p msgcodegen --offline -- --check` | 604 |
| `08-verify-sha256sums.txt` | `python -I tools/evidence-hash/verify_sha256sums.py <probe dir>` | 两探针 GREEN 5/5 + 6/6；改坏 1 处必红、还原 GREEN |

## 复现（离线，不占端口）

```sh
cd <repo>
dotnet run --project mir2-rs/tests/parity/m3/map-effect-probe -c Release -- .
cd mir2-rs
cargo test -p mir2-world --test map_effect_vectors --offline     # 7 passed
cargo test -p mir2-world --test magic_effect_vectors --offline   # 6 passed
```

## 结论摘要

- (a) 离线可做并已产真身向量：**8 / 37（`MagPushArround`）、21（`MagSaceMove`）、46（`MagMakeClone`）**。
- (c) 仍需真召唤（转后续）：**17 / 30 / 41**（`SystemShare.WorldEngine.RegenMonsterByName`，
  点见 `map-effect-probe/vectors/context_reachability.tsv` 的 `missing_prereq`）。
- 唯一替身：21 的 `SystemShare.MapMgr`（只 `FindMap` 字典）；效果路径全真身，报告 §4 显式分栏。

## 残留物

- 临时物：`E:/tmp/m3cs-map-target`（隔离 target）、`E:/tmp/m3cs-magic-append.rs`、`/tmp/cr.bak`；
  探针临时地图目录由 `finally` 清理。仓库内 `git status` 仅含本片交付路径。
