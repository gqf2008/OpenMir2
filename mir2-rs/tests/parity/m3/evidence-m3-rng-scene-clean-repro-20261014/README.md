# 证据包 —— DoD⑤「干净检出可复现」显式机判（2026-10-14）

- 卡：`openmir2-m3-rng-trace-scene-groups`
- 片：DoD⑤ 的「**证据四件套（SHA256SUMS 仓库 LF/git-blob 口径 + 干净检出可复现）**」之**干净检出**半边
- base = origin/master = `7caf974547d013813bf89e443d486f407e8ee04e`
- code_head = `7caf97454`（**零 `.rs`、零代码改动**；本片不改任何被跟踪文件的内容，只新增本证据目录）
- evidence_head = 本 pack 所在提交（= walgit `patch` 条目 head；落笔后钉全 oid）
- 写集：`mir2-rs/tests/parity/m3/**`；零起栈、零端口、零 merge、零自签

## 为什么有它

本卡离线面（①R-6 folded 映射 ②同场景夹具+前置门+`-SelfCheckNoStack` ③`drop`/④兼容 README）
已合入 master `d6443c03f`，此前多轮的复验**都在作者自己的 worktree 里做**（tracked-clean 的既有检出）。
DoD⑤ 明确要求证据包在**干净检出**上可复现 —— 这是本仓库最常踩的坑（工作树字节 CRLF ≠ 仓库字节 LF，
已出现 ≥5 次，见 `tools/evidence-hash/README.md`）。本片**首次**把这条判据显式机判并留痕：
在**新鲜 detached worktree** 上对**本卡全部 16 个证据包**跑 `verify --check-extra`、
对整个 `tests/parity/m3` 跑 `eolcheck`。

## 方法（可复现）

```sh
# 1. 干净检出（不碰共享主检出，也不碰作者 worktree；owner 2026-10-13 04:35 认可的口径）
git worktree add --detach E:/tmp/m3bag-clean 7caf974547d013813bf89e443d486f407e8ee04e
# 2. 在干净树里逐包复核（默认仓库字节口径）+ 多件牙齿
#    目标集 = 本卡证据包：evidence-m3-rng-trace-scene-groups-* / evidence-m3-rng-scene-anchor-* / evidence-m3-rng-scene-obs-closure-*
python -I tools/evidence-hash/verify_sha256sums.py verify <pack> --check-extra
# 3. 整个 m3 树的 EOL trap 判据
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3
```

## 件表

| 件 | 内容 |
| --- | --- |
| `00-commands.txt` | 本片全部命令 + 口径 |
| `01-clean-worktree-head.txt` | 干净检出 head = `7caf974547d013813bf89e443d486f407e8ee04e`（detached，tracked-clean） |
| `02-packs-verify-check-extra.txt` | 本卡 **16 个**证据包 `verify --check-extra` 在干净树里 **全 GREEN、`PACKS_FAIL=0`**（含 obs-closure 16/16、trace-seed-gate 14/14、scenegroupdiff 22/22、…） |
| `03-eolcheck-m3-tree.txt` | `eolcheck mir2-rs/tests/parity/m3` = **EOLCHECK GREEN 3363/3363**（盘上字节与 git blob 在 LF 上一致） |
| `04-selfcheck-nostack.txt` | `run_rng_trace_pair.ps1 -SelfCheckNoStack` = **SELFTEST ok=37/37 rc=0**（在 master `7caf97454` 上重跑） |
| `05-fixture-chain-gate.txt` | `fixture_chain_gate.py check` = **FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0** |
| `06-other-selftests.txt` | 5 个判据脚本 selftest 全 `RC=0`：`rng_consumer_groups` 15/15、`rng_ledger_diff` 37/37、`rng_scene_gate` 22/22、`check_window_batch` GREEN、`check_anchors` 7/7 |
| `07..10-gates-*.txt` | 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3bag-target`）：fmt rc=0 / clippy rc=0(`-D warnings`) / workspace test **101 suites 1200 passed 0 failed 22 ignored** / msgcodegen 604 rc=0 |
| `11-boundary.txt` | 净差量全在本 pack 目录内（`*.rs`=0，冻结面空，base 是 head 祖先） |
| `SHA256SUMS.txt` | 仓库 LF/git-blob 口径 |

## 结论

DoD⑤ 的「干净检出可复现」半边**已机判成立**：本卡 16 个证据包在新检出上 `--check-extra` 全绿，
整个 m3 树无 EOL trap。⇒ 本卡**剩余唯一未达 DoD 仍是卡级 R 真机**：
同场景含怪两相 trace + `groupdiff --fold --strict` 的 site/order/折叠计数 diff=0；
其生产者（两侧 `<side>-scene.json` + `<side>-rng-stream.tsv`）落在
`crates/world/**` + `bins/game-svr/**` + `src/**`（= 本卡 10:35 派单写集禁令），规格已入库
`rng-trace-scene/DRIVER-CONTRACT.md`；承接卡 `openmir2-m3-rng-scene-driver` 未开，窗口归 owner 独占。

## 残留物

- 干净检出 `E:/tmp/m3bag-clean`（detached `7caf97454`）：复核完成后 `git worktree remove` 已清（命令见 `00-commands.txt`）。
- 本 pack 之外**零改动**；共享主检出只读未动；未起栈、未占端口。
