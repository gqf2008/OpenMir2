# 证据包 `evidence-m3-quest-load-20261013`

卡 `openmir2-m3-quest-load`（M3⑥-实现 1：`Envir/MapQuest.txt` 加载 + 隐藏任务 NPC）。
写集 = `mir2-rs/crates/envir/**` + 本证据；零 `crates/world`／`bins`／`src`／`sql`。

## 文件

| 文件 | 内容 |
| --- | --- |
| `01-deployment-input.txt` | 部署件 `E:/MirServer/M2GameSvr/Envir/MapQuest.txt` 原始件（`ls -la` + 行形态）+ 三个隐藏脚本存在性（`RobotManage.txt` 缺） |
| `02-deployment-readout.txt` | 真机向量实跑：`cargo test -p mir2-envir --test mapquest -- --ignored --nocapture`（`MIR2_ENVIR_DIR` 指向部署件） |
| `03-gates-fmt.txt` | `cargo fmt --all -- --check`（空 = rc 0） |
| `04-gates-clippy.txt` | `cargo clippy --workspace --all-targets -- -D warnings` |
| `05-gates-test.txt` | `cargo test --workspace --no-fail-fast`（91 suites / 822 passed / 0 failed / 19 ignored） |
| `06-gates-msgcodegen.txt` | `cargo run -p msgcodegen -- --check`（604） |
| `07-mutation-red.txt` | 改坏必红两次（汇总 + 还原后 GREEN） |
| `07a-mutation-red.txt` | 变异① 原始输出（`nValue>1`→`>=0` ⇒ `parses_deployed_line` RED） |
| `07b-mutation-red-group.txt` | 变异② 原始输出（`compare_lstr`→精确相等 ⇒ `synthetic_vectors` RED） |
| `07c-restore-green.txt` | 还原后 `cargo test -p mir2-envir --lib mapquest` GREEN |
| `window/00-ports.txt` | 端口快照（未起栈、未占端口；仅共享 3306） |

## 复核

```sh
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-quest-load-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-quest-load-20261013
```
