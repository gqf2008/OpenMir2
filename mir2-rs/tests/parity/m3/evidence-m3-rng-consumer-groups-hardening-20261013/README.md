# 证据包：M3 Claim B —— 消费者组账本 reviewer-5 O-1/O-2/O-3 收口（2026-10-13）

- 卡：`openmir2-rng-consumer-structure-parity`（owner 派单 comment `d1135a73`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-consumers`
- 上一被审 head = `8afb97fe`（reviewer-5 `ed51dd62` approve，3 条非阻塞 Minor O-1/O-2/O-3）；
  本片 = 该 approve 的**收口片**：把三条 Minor 落成 fail-closed 牙 + 口径文档。
- base = `refs/heads/master` `d778ac30`（已 `git merge origin/master`）。报告：
  `../M3全服RNG消费者结构一致性基线报告-20261013.md`

## 为什么有这一包

reviewer-5 的 approve 给了三条非阻塞 Minor，本片逐条收口（**不放松任何既有判据**）：

- **O-1（observability-gap）**：`iter_sites` 对解析不了的行（列<4、首列非整数）**静默 `continue`**
  且不计入 `rows_total` ⇒ `GROUP_COVERAGE closed` 只证明「已解析行 100% 归类」，**不证明**文件里没有被
  吞掉的行。**收口**：新增 `stats["rows_dropped"]/["rows_kept"]` 行级覆盖计数；`groups --require-closed`
  与 `groupdiff --strict` 对 `rows_dropped>0` **fail-closed**（`ROW_COVERAGE DRIFT` / `ROW_COVERAGE_DRIFT`）。
- **O-2（criterion-scope）**：口径文档明写「**组内 draw 计数本身不是判据**，只有组集合与共享组相对次序是」。
- **O-3（table-validation-scope）**：`validate()` 原本只对 `csharp` 强制 CORE 四组；AUX 组整组被删抓不到。
  **收口**：`validate()` 对 `csharp` **同时**强制 CORE + AUX（`construct/npc_ai/revive/homemap/startup`）。

## 件清单

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 全部可复算命令（自检 / 基线 / 改坏必红 / 四门禁 / 复核） |
| `01-selftest-rng-ledger-diff.txt` | `rng_ledger_diff.py selftest` → **25/25 GREEN**（原 20/20；+5 条 O-1/O-3） |
| `02-selftest-attack-rng-ledger.txt` | `attack_rng_ledger.py selftest` → **32/32 GREEN**（原 31/31；+1 条委派 O-1） |
| `10-baseline-csharp-groups.{txt,json}` | C# 活栈（F-1 权威件）分组读数：`rows_total=5090894 rows_dropped=0`，`ROW_COVERAGE closed` |
| `11-baseline-rust-groups.{txt,json}` | Rust 活栈分组读数：`rows_total=160 rows_dropped=0`，`ROW_COVERAGE closed` |
| `12-baseline-groupdiff.{txt,json}` | 两侧组集合 + 相对次序 diff（`ROW_COVERAGE a_dropped=0 b_dropped=0`） |
| `13-mutation-rowcoverage-red.txt` | **O-1 改坏必红**：全部站点已归类（`closed`）但 2 行解析不了 ⇒ `ROW_COVERAGE DRIFT` + rc=1 |
| `14-mutation-real-table-drift-red.txt` | **O-1 真机对照**：删 `drop/csharp` 行 ⇒ 真实 C# trace `unclassified=410383` + rc=1 |
| `15-mutation-aux-table-red.txt` | **O-3 改坏必红**：删 `revive/csharp` 行 ⇒ `validate()` 点名（原表 `validate()==[]`） |
| `20-gate-fmt.txt` … `23-gate-msgcodegen.txt` | 四门禁原始输出（93 suites 847 passed 0 failed 20 ignored + 604） |
| `24-boundary.txt` | 边界：`src sql MirClient` / `mir2-rs/crates bins` / `**/*.rs` / `tests/parity/world` 全空 |
| `fixtures/syn-rowcoverage.tsv` | 合成 trace：1 行可归类 + 2 行解析不了（O-1 fixture） |

## 判据读数（摘要）

- 自检：`25/25`、`32/32` GREEN；组表 `validate()` 空。
- O-1 牙：`syn-rowcoverage.tsv` 上 `GROUP_COVERAGE closed` **但仍** `ROW_COVERAGE DRIFT rows_dropped=2` + rc=1
  ⇒ 「closed 不能掩盖被吞掉的行」；真实两侧 trace `rows_dropped=0`（非假绿）。
- 真机表漂移对照：删 `drop/csharp` 行 ⇒ C# trace **410,383 未归类** + rc=1（reviewer 独立牙的复现）。
- O-3 牙：删 `revive/csharp` 行 ⇒ `validate()` 非空并点名。
- 基线（**未变**）：C# 非攻击 **5,090,542** draws / 9 组 vs Rust **14** draws / 2 组；**7 组只在 C#**
  （合计 5,090,528 draws）；两侧未归类 0、行级覆盖 0。次序差仍是**场景差异**，不作实现缺陷判据
  （口径诚实性见报告 §2.2；O-2 已在报告/工具 docstring 明写「计数非判据」）。

## 口径

- `SHA256SUMS.txt` 按**仓库字节（LF 归一 / git blob）**记；复核：
  `python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-rng-consumer-groups-hardening-20261013`。
- 仓外原始件（`E:/tmp/m3cs-*`）未入库，按 sha256 引用（报告 §2/§6）。
- **未起栈、未占端口**：本片纯离线（读写 trace / 跑 python / 跑门禁），十端口状态与开工前一致（仅共享 3306）。
