# 证据包 —— 站点表 `oracle_ref` 可解析性机判（2026-10-13 第三次窗口前置审计）

卡：`openmir2-m3-rng-trace-scene-groups`（m3bag / `openmir2-worker-4`）
分支：`wip/m3bag-window-batch-premise`
base：`origin/master` = `ad0592b19`（本卡前两片——Claim B 离线收口 `05c3b4283` 与锚点保鲜 `6ecdae644`——均已作为 master `ad0592b19` 合入；本片 rebase 到其上再交，避免「base 未含 master ⇒ 回退假差量」）
tier：**L**（纯离线表/工具/文档片，零 `.rs`）；卡级真机 DoD 仍 **R / not_verified**。

## 1. 本片修什么（真缺陷，不是空转）

卡的**离线①②③ 均已达成并入 master**；剩余 DoD = **真机两侧两相 trace + `groupdiff --fold --strict`**，
阻塞在两张卡外条件上（承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口）。本 burst 继续
**窗口前置漂移审计**（与上一片 `evidence-m3-rng-scene-anchor-freshness-20261013` 同族），逐项核对
判据链上的**溯源锚点**，发现一个会误导承接卡/评审的真缺陷：

`consumer_groups.tsv` 是 Claim B 的**唯一真源**，其 `oracle_ref` 列是站点归因的唯一溯源
（`pattern` 判组，`oracle_ref` 供人复查）。该列此前只校验「非空」⇒ 指向不存在源码路径的 ref
能静默通过：

- `startup/csharp` 行写 `src/SystemModule/Conf/ServerConf.cs:114`，
  实际文件在 **`src/Modules/SystemModule/Conf/ServerConf.cs`**（缺 `Modules/` 段）。
- 该行**第 114 行确实是** `SystemShare.Config.ItemNumber += SystemShare.RandomNumber.Random(10000);`
  ⇒ 行号对，**路径错**；承接卡/评审按它去源码里找会找空，且**没有任何门会拦**。

⇒ 本片修路径 + 把「能解析成源码文件的 `oracle_ref` 必须**文件存在**且**行号在范围内**」钉成
fail-closed 机判（不可解析的 ref——glob、目录、Rust 模块入口无行号——按口径跳过）。

## 2. 交付物

| 文件 | 变更 |
| --- | --- |
| `rng-trace-probe/consumer_groups.tsv` | `startup/csharp` 的 `oracle_ref`：`src/SystemModule/Conf/ServerConf.cs:114` → **`src/Modules/SystemModule/Conf/ServerConf.cs:114`** |
| `rng-trace-probe/rng_consumer_groups.py` | **新增** `resolve_oracle_ref_path` / `oracle_ref_line` / `validate_oracle_refs`；`selftest` 12/12 → **15/15**（含坏路径 / 越界行 2 条改坏必红） |
| `rng-trace-scene/run_rng_trace_pair.ps1` | `-SelfCheckNoStack` 新增 `station-table-refs-selftest` ⇒ 自检 **30/30 → 31/31** |
| `rng-trace-scene/README.md` | 新增 §5.3（口径 + 命令 + 改坏必红）+ §6 命令清单 |

路径口径（与仓库布局一致，避免把相对 `mir2-rs/` 的 Rust 路径误报成缺件）：

- `src/...` → 相对**仓库根**（C# oracle）；
- `crates/...` / `bins/...` → 相对 **`mir2-rs/`**（Rust 站点）；
- `mir2-rs/...` → 相对**仓库根**。

`validate_oracle_refs` **不改** `validate()`（保持运行时行为逐字节不变，也不影响 `rng_ledger_diff`
用内存合成表的 selftest）；只作为独立机判供 selftest / 无栈自检调用。

## 3. 判据读数（全 GREEN / 改坏必红真跑）

| 项 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 |
| --- | --- | --- |
| **改前复现**（`git show HEAD:` 取出的旧表 + 新机判） | 见 `01-audit-before-red.txt` | **问题数 = 1**，`RED 行28(startup/csharp): oracle_ref 指向不存在的源码文件 src\SystemModule\Conf\ServerConf.cs` |
| **改后**（工作树新表 + 新机判） | 见 `02-audit-after-green.txt` | **问题数 = 0**（可解析 ref 29/30 行；余 1 为 `Monsters/*.cs` glob，按口径跳过） |
| 机判（新门） | `python -I rng-trace-probe/rng_consumer_groups.py selftest` | **15/15 GREEN**（含 `mutation-oracle-ref-path-red` / `mutation-oracle-ref-line-red` 2 条改坏必红，见 `03-…`） |
| 无栈自检（接入后） | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | **31/31 GREEN**（含 `station-table-refs-selftest`；本片之前 30/30），见 `04-…` |
| 离线全链回归 | 见 `05-offline-regression.txt` | `rng_consumer_groups 15/15`、`rng_ledger_diff 37/37`、`rng_scene_gate 22/22`、`check_window_batch 13/13`、`check_evidence_counts 7/7`、`check_anchors 4/4`、`attack_rng_ledger 32/32`、`drop_exp_purity_gate 23/23`、`drop_exp_diff 12/12` 全 rc=0 |
| 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`, cwd=mir2-rs） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1199 passed 0 failed 22 ignored** / 604 rc=0（见 `06`–`09`） |

> **改坏必红（真跑，非叙述）**：`mutation-oracle-ref-path-red` 把该行 ref 改成
> `src/NoSuchModule/Conf/ServerConf.cs:114` ⇒ 机判点名「指向不存在的源码文件」；
> `mutation-oracle-ref-line-red` 把它改成 `…ServerConf.cs:999999` ⇒ 机判点名「行 999999 越界
> （共 682 行）」。两条都含在 `03-selftest-consumer-groups.txt`。

> 四门禁 / 无栈自检在 **rebase 到 `ad0592b19` 之后**的 head 上重跑（读数见 `06`–`10`），与 rebase 前一致。

## 4. 边界

`git diff --name-only ad0592b19 HEAD` 全部在**本卡写集**内：

- 源：`rng-trace-probe/consumer_groups.tsv`、`rng-trace-probe/rng_consumer_groups.py`、
  `rng-trace-scene/README.md`、`rng-trace-scene/run_rng_trace_pair.ps1`
  （+ 上一片的 `rng-trace-scene/DRIVER-CONTRACT.md`、`check_anchors.py`）；
- 证据：本目录 `01`–`10` + `README.md` + `SHA256SUMS.txt`（上一片另有独立证据目录）。

冻结面（`src/`、`sql/`、`MirClient`、`mir2-rs/crates/protocol`、`mir2-rs/crates/storage`、`mir2-rs/bins`）
**空**，`*.rs` = 0。**未动** `crates/world`、`bins/**`、`src/**`、`tests/parity/world/**`、drop-exp 实现。
工作树里 8 个未跟踪 `sessions/csharp-20261012-23xx/` 是**他卡遗留**，未并入本片。

## 5. 残留 / 登记

- 卡级 DoD（真机两相 trace + `groupdiff --fold --strict`）**未做**：唯一阻塞为
  ① 承接卡 `openmir2-m3-rng-scene-driver` 未开（两侧 `<side>-scene.json` + `<side>-rng-stream.tsv`
  生产者在 `crates/world/**` + `bins/game-svr/**` + `src/**`，超出本卡写集禁令）；
  ② owner 独占窗口（与本卡真机两相 + ④ 真机 100 杀同批次串行）。
- **仅登记、未修**（纪录片，不阻塞判据）：`consumer_groups.tsv` 的 `oracle_ref` 行号语义是宽口径
  「方法附近」，并非统一「首个抽数行 / 声明行」；本片只强制「文件存在 + 行号在范围内」，
  不强制行号精确指向抽数点。若 owner 要收严，另开一条把口径写成表头规范并逐行对齐。
- 未 merge、未自签 review；零起栈、零占端口。
