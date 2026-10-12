# evidence-m3-npc-shop-inventory-20261013

卡 `openmir2-m3-npc-shop-align` 的 DoD① 现状盘点证据（在 `worktree-m3-npc-shop-align`，base = `origin/master 384379fb`）。

**本片只做盘点**（不占窗口、零 `.rs`、未起栈、未占端口）。报告：
[`../M3⑤NPC商店现状盘点报告-20261013.md`](../M3⑤NPC商店现状盘点报告-20261013.md)。

## 文件

| 文件 | 内容 | 生成命令 |
| --- | --- | --- |
| `00-oracle-chain-lines.txt` | oracle 调用链行号锚点（`grep -n`；消息号/收包派发/玩家处理/商人逻辑/脚本层/加载/落库 + 真实 `MerChant.txt` 与 `Market_Def` 脚本头） | 见文件内 `grep` 命令 |
| `01-rust-status-greps.txt` | Rust 现状只读 grep：bins/game-svr 无 NPC/商店 `CM_*` 派发、envir/world 无商人实体、无价格公式、protocol 只有常量 | 见文件内 `grep` 命令 |
| `02-m4-npc-cases-crossref.txt` | M4 7/7 NPC 用例（脚本层）与本卡派发面的对表 + M4 报告 §5-1 摘录 | `tests/parity/M4/npc-cases.json` + `evidence/M4/*` |
| `03-fixture-reuse-survey.txt` | `run_pair.ps1` 参数面 / `scoped_diff.py` / `gate_guards.ps1` 的夹具可复用性调查 | 见文件内 `sed/grep` 命令 |
| `04-gate-fmt.txt` | 门禁：`cargo fmt --all -- --check` | rc=0 |
| `05-gate-clippy.txt` | 门禁：`cargo clippy --offline --workspace --all-targets -- -D warnings` | rc=0 |
| `06-gate-test.txt` | 门禁：`cargo test --offline --workspace` = 84 suites / 687 passed / 0 failed / 16 ignored | rc=0 |
| `07-gate-msgcodegen.txt` | 门禁：`cargo run --offline -p msgcodegen -- --check` = 604 条 | rc=0 |

## 复现（离线，零端口）

```bash
# 四门禁（隔离 target，--offline）
CARGO_TARGET_DIR=E:/tmp/m3cs-target cargo fmt --all -- --check
CARGO_TARGET_DIR=E:/tmp/m3cs-target cargo clippy --offline --workspace --all-targets -- -D warnings
CARGO_TARGET_DIR=E:/tmp/m3cs-target cargo test --offline --workspace
CARGO_TARGET_DIR=E:/tmp/m3cs-target cargo run --offline -p msgcodegen -- --check

# 证据清单（仓库字节口径 / LF 归一 / git blob；README.md 本身不列入清单 ⇒ --check-extra 需 --exclude 排除它）
python -I tools/evidence-hash/verify_sha256sums.py --check-extra --exclude README.md mir2-rs/tests/parity/m3/evidence-m3-npc-shop-inventory-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-npc-shop-inventory-20261013
```

## 关键结论（详见报告）

- **脚本层已就绪**：`crates/script` 解析商人脚本头（`(@buy @sell …)`、`%PRICERATE`、`+StdMode`）与 `[goods]`
  （`crates/script/src/parser.rs:381/522`、`model.rs:97/111`）；M4 7/7 只覆盖脚本层。
- **服务端派发面整个缺件**：`crates/envir` 无 NPC/商人加载、`crates/world` 无 NPC 实体/商人 tick、
  `bins/game-svr` 无 `CM_MERCHANTDLGSELECT`/`CM_USERBUYITEM`/… 派发与价格/库存/仓库态。
- **两个必须处理的时序坑**：① 商人 `GoodsList` 初始为空、只在 `Merchant.Run` 的 5 分钟门后补货且不落盘
  （夹具宜用「卖后回购」自造货架）；② `ClientUserBuyItem` 用 `<=15`、`IsWithinSight` 用 `<15`（距离口径不一致）。
- **夹具可复用 `run_pair.ps1` 的两相固定起点**（DB `scoped_diff` + 会话入库），只需新增 BotSrv 商店动作序列 +
  `npc_shop_diff.py` + `run_npc_shop_pair.ps1`。

**残留物**：本目录 8 件 + 本 README + `SHA256SUMS.txt`；未起栈、未占端口、未动 `crates/**`/`bins/**`/`.ps1`。
