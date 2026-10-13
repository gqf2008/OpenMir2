# evidence-m3-rng-host-mongen-row-20261013

卡：`openmir2-m3-rng-trace-scene-groups`（m3bag / openmir2-worker-4）
片：**窗口前置审计 —— Claim B 组表漏登宿主 `bins/game-svr/src/mongen.rs` 的构造期抽数**
日期：2026-10-13（owner 独占窗口发放前；纯离线 / 零起栈 / 零端口 / 零 `.rs`）

## 1. 发现了什么（为什么这会在共用窗口里空转一轮）

卡 DoD①/②的判据是 `rng_ledger_diff.py groupdiff --fold --strict --scenario <scenario>`：
两侧账本的**组集合 + 相对次序 + R-6 折叠计数**必须全等。判据的组 vocabulary 由
`rng-trace-probe/consumer_groups.tsv`（唯一真源）定义；**未登记的站点按 `other` 计**，
而 `other` 只出现在一侧 ⇒ `GROUP_SET_DIFF`（`--strict` 判红）。

固定场景（`rng_trace_scenario.json`：图 `0137`、MonGen **0 行**、只 pin 一只稻草人）
下，Rust 侧的 `spawn` 组**唯一来源** = 这只 pin 怪的**宿主构造抽数**，落在
`bins/game-svr/src/mongen.rs`（`create_monster_oracle` → `draw_monster_construct` →
`draw_base_and_animal` 等，实测 `rng.random_below` 调用 75 处）。而表里当时只有
`crates/world/src/mongen.rs`（刷怪/复活；MonGen 0 行 ⇒ 本场景不抽）⇒ 这批抽数会被算成
`other`，**同时** DoD② 声明的 `spawn` 组在 Rust 侧缺失 ⇒ `MISSING_EXPECTED_GROUPS(spawn)`。
即：不修就必然在窗口里拿到一次假红（且会以为「实现不一致」）。

对照：④ 的 `drop-exp/drop_exp_consumers.tsv:52` **早已**把 `bins/game-svr/src/mongen.rs`
列为 rust `setup` 行 —— 两张表口径不一致，Claim B 这侧是漏登记。

## 2. 改了什么（写集 `mir2-rs/tests/parity/m3/**`，零 `.rs`）

| 文件 | 改动 |
| --- | --- |
| `rng-trace-probe/consumer_groups.tsv` | **表末追加**一行 `spawn rust bins/game-svr/src/mongen\.rs:`（追加在末尾以免移动上方锚行行号：`tsv:35` / `tsv:53` 仍是原锚）；附 8 行注释说明依据 |
| `rng-trace-probe/rng_consumer_groups.py` | 新增 `__main__` 离线自检（12/12）：表自洽 + 逐站点归类（含**盘上 `\` 归一**）+ **删除宿主行 ⇒ 必红** + 删 CORE 组 ⇒ validate 点名。此前 `python ... rng_consumer_groups.py selftest` 是**空跑 rc=0**（无 `__main__`）⇒ 本片把它变成真门 |
| `rng-trace-scene/DRIVER-CONTRACT.md` | §6 补一条：宿主 mongen 构造抽数归 `spawn`，机判入口写清 |

# 3. 判据读数（证据件，全部本片实跑）

| # | 件 | 判据 | 读数 |
| --- | --- | --- | --- |
| 01 | `01-drift-repro-before.txt` | 表缺行 + `groups --require-closed` | **rc=1** `UNCLASSIFIED bins/game-svr/src/mongen.rs:289/290`（改前复现） |
| 02 | `02-drift-fixed-after.txt` | 表含行 + 同命令 | **rc=0** `GROUP_COVERAGE closed`（3/3 `spawn`） |
| 03 | `03-selftest-rng-consumer-groups.txt` | `rng_consumer_groups.py selftest` | **12/12 GREEN**（含两条改坏必红） |
| 04 | `04-selftest-rng-ledger-diff.txt` | `rng_ledger_diff.py selftest` | **37/37 GREEN**（未回归） |
| 05 | `05-groupdiff-identical-synthetic.txt` | 场景形合成对 `groupdiff --fold --strict --scenario` | **VERDICT IDENTICAL**（折叠计数 `spawn:3` 两侧相等） |
| 06 | `06-mutation-host-row-removed-red.txt` | 同上 + 表去行 | **rc=1** `GROUP_SET_DIFF+SHARED_REL_ORDER_DIFF+FOLDED_COUNT_DIFF`（改坏必红） |
| 07 | `07-static-audit-rust-rng-files.txt` | 静态审计：生产路径 RNG 文件 vs 两表覆盖 | 唯一未登记 = 本行；其余未覆盖文件（magic/slave/script）固定场景不触发（如实列出，不擅自归类） |
| 08–11 | `08..11-gates-*.txt` | 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`） | fmt rc=0 / clippy rc=0(`-D warnings`) / test **101 suites / 1198 passed / 0 failed / 22 ignored** / msgcodegen 604 rc=0 |
| 12 | `12-boundary.txt` | 净差量 + 冻结面 | 见文件（全在 `mir2-rs/tests/parity/m3/**`；`src/ sql/ MirClient/ mir2-rs/crates mir2-rs/bins` 空，`*.rs` = 0） |

场景形合成对（`fixtures/csharp-scene-like.tsv` / `rust-scene-like.tsv`）只为**离线证明表行承重**，
不是真机 trace；真机两相 trace 仍 `not_verified`（需承接卡 `openmir2-m3-rng-scene-driver` + owner 独占窗口）。

## 4. tier

**L**（离线表/工具片）：零 `.rs`，不改任何 RNG 抽数序列 / 播种 / 时序 / 落库 / 战斗数值；
只把「已知站点归到哪一组」这一**判据表**补全并加机判门 ⇒ 不可能只在真机暴露差异（真机只会确认归类）。
**卡级 DoD 仍是 R**（同场景含怪两相 trace 的 site/order 账本 diff=0）⇒ `not_verified`，本片不替它断言。

## 5. 纪律 / 残留

- 未 merge、未自签 review（作者 `openmir2-worker-4` ≠ reviewer）；只在自己 worktree 写；
  共享主检出 `E:\Users\gxh\Documents\GitHub\OpenMir2` 零改动。
- 零起栈 / 零占端口（判据全离线）；未动 `crates/world`、`bins/**`、`src/**`、`tests/parity/world/**`。
- 残留：`E:/tmp/_drift_probe.tsv`、`E:/tmp/m3bag-drift-audit/**`、`E:/tmp/_t_m3bag.json`、`E:/tmp/_cg_master.tsv`
  为本轮临时件（已登记，可随时删）；复用 target `E:/tmp/m3bag-target`（本车道）。
- 工作树未跟踪的 `sessions/csharp-20261012-23xx/`（8 个）为**他卡遗留**，未并入本片。
