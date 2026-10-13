# evidence-m3-quest-state-20261013

卡 `openmir2-m3-quest-state`（M3⑥-实现 2）证据包。全部命令在**本 worktree** 内、`CARGO_TARGET_DIR=E:/tmp/m3cs-target`。

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-oracle-anchors.txt` | `src/` 实测行号：位图字段/六入口、两个条件、注册表/引擎调用点、`WorldServer` 载入/写回（写集边界） |
| `01-probe-run.txt` | 真身探针命令 + 输出（bit rows 96 / condition rows 8）+ 向量清单 + 条件真值表 |
| `02-gates.txt` | 四门禁读数（fmt/clippy/test/msgcodegen） |
| `03-redcheck-break-must-be-red.txt` | 改坏必红：位方向 `128>>n → 1<<n` ⇒ RED，还原 ⇒ GREEN |
| `04-rust-status-greps.txt` | 改前（`git grep HEAD` = no match）vs 本片（分支/派发登记）+ trait 实现点 |

向量本体在 `mir2-rs/tests/parity/m3/quest-state-probe/vectors/`（与探针同目录，另有其 `SHA256SUMS.txt`）。

## 口径

`SHA256SUMS.txt` 按**仓库字节（Git blob / LF 归一）**记，复核命令：

```sh
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-quest-state-20261013
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/quest-state-probe
```

## 复现

```sh
# 探针（真 PlayObject + 真 ConditionProcessingSys）
dotnet run --project mir2-rs/tests/parity/m3/quest-state-probe -c Release -- mir2-rs/tests/parity/m3/quest-state-probe/vectors
# Rust 向量核对 + 条件真值表
cargo test -p mir2-script --test quest_state_vectors
cargo test -p mir2-script --test engine_tests -- condition_truth_table_matches_oracle checkunit checkopen
```

报告：`mir2-rs/tests/parity/m3/M3⑥任务态位读写与CHECKOPEN-CHECKUNIT对齐报告-20261013.md`。
