# M3 门禁脚本健康清单（gate-health）— 2026-10-17

卡：`openmir2-fixture-chain-gate-red-at-baseline`。基线：`a18586f39`（master tip）。
用途：查清「`fixture_chain_gate.py check` **在基线就 RED**（rc=1）」的根因，并把
`m3/*gate*.py` + `m3/rng-trace-scene/*.py` 下**所有**带 `check` / `selftest` / `mutation`
（任务文里写作 `mutate`）子命令的脚本逐个跑一遍，标出「**基线就红（既存债）**」与「**本轮新红**」两类。
价值：回答「门禁清单里有多少条是长期红的」。

## 0. 结论摘要

- 基线 `a18586f39` 上，`fixture_chain_gate.py check` = **RED `red=3`**，三条**同根**：全部源自
  `m3/rng-trace-scene/check_anchors.py` 的 **2 项 RED**（`ANCHOR_DRIFT` + `BARE_REF_DRIFT`）。
- 定性：**两条都是 ①「锚漂移」**（行号过期），**不是 ②「真缺口」**（锚定的事实没变）。
  根因 = `f1cffd3f1`（片117）改写 `consumer_groups.tsv` 表头注释块，**净增 3 行** ⇒ 其下所有行 +3。
- 修法 = 按本仓口径**修锚**（行号锚，漂移 +3 在 ±3 内命中声明符号）：文档 `:35→:38`、`:53→:56`，
  同步锚表 `also.line=53→56` 与 selftest 硬编码。**判据不降级**（不动 `rx`、不放宽）。
- 修后：`check_anchors.py check` GREEN 7/7、`selftest` GREEN 17/17、`fixture_chain_gate.py check`
  GREEN `red=0`。
- **清单核查**：48 条具名子命令中，**基线唯一长期红的就是 `check_anchors.py` 的 check/selftest（1 根因）**；
  其余 `rc=0`（GREEN）或 `rc=2`（**N/A-INPUT**：argparse 缺必填入参，需喂夹具/证据目录，非门失败）。
  **本轮新红 = 无**。

## 1. 三条 RED 的逐条定性

`fixture_chain_gate.py check` @ `a18586f39`：

```
FIXTURE_CHAIN RED: runner_ok=9 prereq_or_skip=0 red=3
  RED B 全服 RNG 消费者账本（Claim B）：runner -SelfCheckNoStack rc=1（SELFTEST ok=36/37 failures=anchors-freshness-selftest）
  RED 共享管线 rng_scene_anchor_selftest：.../rng-trace-scene/check_anchors.py rc=1（SELFTEST RED: check_anchors selftest 16/17）
  RED 共享管线 rng_scene_anchor_check：.../rng-trace-scene/check_anchors.py rc=1（RED: check_anchors 2 项 RED）
```

| # | RED 行 | 直接来源 |
| --- | --- | --- |
| 1 | `B` 系统 runner `-SelfCheckNoStack` rc=1（`36/37`） | `run_rng_trace_pair.ps1` 的 `anchors-freshness-selftest` 子项 = `check_anchors.py selftest` |
| 2 | 共享管线 `rng_scene_anchor_selftest` rc=1（`16/17`） | `check_anchors.py selftest` 的 `real-doc-fresh` 用例 |
| 3 | 共享管线 `rng_scene_anchor_check` rc=1（`2 项 RED`） | `check_anchors.py check` 本体 |

⇒ 三条同根，收敛到 `check_anchors.py` 的 2 项 RED。

### 1.1 两条 RED：锚期望值 vs 实际值

（1）`ANCHOR_DRIFT` — 站点表的 R-6「折叠理由」行

- 锚：`literal=rng-trace-probe/consumer_groups.tsv`，`rx=FOLD_RULES = \{construct: spawn\}`（取首个匹配行）。
- 文档 `DRIVER-CONTRACT.md:98` 写：`consumer_groups.tsv:35`（折叠理由）。
- 实际：`/FOLD_RULES = \{construct: spawn\}/` 首个匹配在 **`:38`**。漂移 **+3**。
- 实际行内容（`:38`）：`#   但要求**机判等价** —— `rng_consumer_groups.FOLD_RULES = {construct: spawn}`（两侧同折），`

（2）`BARE_REF_DRIFT` — Rust `spawn` 行（同行裸续引用 `:NN`）

- 锚 `also`：`line=53`，`rx=^spawn\trust\tcrates/world/src/mongen`。
- 文档同句写：`:53`（Rust `spawn` 行）。
- 实际：`:53` = `spawn<TAB>csharp<TAB>RegenMonsters@WorldServer\.MonGen\.cs:<TAB>...`（C# 行）；
  `spawn<TAB>rust<TAB>crates/world/src/mongen\.rs:<TAB>...` 在 **`:56`**。漂移 **+3**。

### 1.2 定性：①锚漂移（非②真缺口）

- **根因**：`f1cffd3f1`「构造点账本收口——12 处 new Merchant 全枚举 + 站点账本刷新」（片117）把
  `consumer_groups.tsv` 表头 `# 注意：Rust 侧 ④-D…`（旧 5 行）改写为
  `# 注意（2026-10-17 订正，片114）…`（新 8 行）⇒ **净增 3 行**，其下所有行 +3。
  （`git show f1cffd3f1 -- .../consumer_groups.tsv`：`10 insertions(+), 7 deletions(-)`。）
- **被锚的事实没变**：`FOLD_RULES = {construct: spawn}` 仍在（`:38`）；`spawn rust crates/world/src/mongen\.rs:`
  行仍在（`:56`）。⇒ 只是行号过期。
- **证伪「真缺口」**：在 `f1cffd3f1~1`（该片之前）的 `consumer_groups.tsv` 上，旧 `:35`/`:53` 逐字命中
  这两个符号 ⇒ 文档当时正确、符号未变、仅行号漂移，属 ①。漂移量 +3 ∈ 行号锚「±3 内命中声明符号」的容忍带。

## 2. 修法（① ⇒ 修锚；判据不降级）

- `m3/rng-trace-scene/DRIVER-CONTRACT.md:98`：`consumer_groups.tsv:35` → `:38`、`:53` → `:56`。
- `m3/rng-trace-scene/check_anchors.py`：
  - 锚 `ANCHORS` 中 `also=（… line=53 …）` → `line=56`（`rx` **不变**）；
  - `selftest` 用例 ⑧/⑨/⑬ 的硬编码 `:53`/`:54` → `:56`/`:57`（改标签，不改判据）；
  - docstring 第 7 条示例同步 `:35/:53` → `:38/:56`。
- **不改判据**：`rx`、`±3` 口径、闭世界、`symbol_unique` 全部不动；未放宽任何期望。
- **为何不把 `consumer_groups.tsv` 改「符号唯一性锚」**：本仓口径下它是**行号锚**文件——
  `DRIVER-CONTRACT.md`「本文档自身的源码锚点保鲜」把 `consumer_groups.tsv` 与
  `mongen.rs` / `monai.rs` / oracle `AnimalObject.cs` / `rng_consumer_groups.py` 同列为**稳定文件**（行号锚），
  只把「多车道共享的增长文件 `bins/game-svr/src/main.rs`」列作符号唯一性锚。本处漂移 +3 在容忍带内 ⇒ 属
  「锚过期」，修锚即可。
- **churn 观察（建议，另立卡）**：本轮真因是**别的车道**（片117）插行 ⇒ 与 main.rs 同族。若
  `consumer_groups.tsv` 再漂一次，宜按 main.rs 先例把「折叠理由行」改**符号唯一性锚**，从根上消掉该族假 RED。

**修后证据**：

```
python -I check_anchors.py check    → GREEN: check_anchors 7/7 锚点保鲜  （rc=0）
python -I check_anchors.py selftest → SELFTEST GREEN: check_anchors selftest 17/17  （rc=0）
```

## 3. 门禁脚本清单核查（`a18586f39` 基线 vs 本轮）

方法：每个脚本先 `--help` 探用法；对每个 `check` / `selftest` / `mutation` 子命令逐一执行；
并在 `a18586f39` 的**干净 detached worktree**（`/e/tmp/fcg-baseline`，`git worktree add --detach`）上
**复跑同一脚本**，对比 rc 列。范围 = `m3/*gate*.py`（14 个）+ `m3/rng-trace-scene/*.py`（5 个，含
`check_*` 与 `rng_scene_gate.py`）+ `m3_whitelist_registry_audit.py`（门禁列点名件）。

| 脚本 | 子命令 | `a18586f39` rc | 本轮 rc | 归因 |
| --- | --- | --- | --- | --- |
| attack_stream_gate.py | gate | 2 | 2 | N/A-INPUT（需 `--stream/--expected`） |
| attack_stream_gate.py | selftest | 0 | 0 | GREEN |
| castle_oracle_anchor_gate.py | check / audit / selftest / mutation | 0 | 0 | GREEN |
| fixture_chain_gate.py | check | 1（RED red=3） | **0（GREEN red=0）** | 见 §4；根因 = check_anchors |
| guild_castle_anchor_gate.py | check / selftest / mutation | 0 | 0 | GREEN |
| hero_pet_anchor_gate.py | check / selftest / mutation | 0 | 0 | GREEN |
| m3_fixture_fmt_evidence_gate.py | check / selftest / mutation | 0 | 0 | GREEN |
| m3_guild_war_quirk_registry_gate.py | check / selftest | 0 | 0 | GREEN |
| m3_pet_slave_registry_gate.py | check / selftest | 0 | 0 | GREEN |
| marker_prefix_gate.py | check / selftest / mutation | 0 | 0 | GREEN |
| party_trade_anchor_gate.py | check / selftest / mutation | 0 | 0 | GREEN |
| quest_anchor_gate.py | check / selftest / mutation | 0 | 0 | GREEN |
| selloff_anchor_gate.py | check / selftest / mutation | 0 | 0 | GREEN |
| server_state_gate.py | --selftest | 0 | 0 | GREEN |
| whitelist_anchor_gate.py | check / audit / selftest | 0 | 0 | GREEN |
| m3_whitelist_registry_audit.py | audit | 0 | 0 | GREEN |
| **check_anchors.py** | **check** | **1** | **0** | **基线就红（既存债）**；本片修 |
| **check_anchors.py** | **selftest** | **1** | **0** | **基线就红（既存债）**；本片修 |
| check_evidence_counts.py | check | 2 | 2 | N/A-INPUT（需 `--report/--dir`） |
| check_evidence_counts.py | selftest | 0 | 0 | GREEN |
| check_evidence_head.py | check | 2 | 2 | N/A-INPUT（需 `--dir/--head`） |
| check_evidence_head.py | check-pack | 2 | 2 | N/A-INPUT（需 `--dir/--since`） |
| check_evidence_head.py | selftest | 0 | 0 | GREEN |
| check_window_batch.py | check / selftest | 0 | 0 | GREEN |
| rng_scene_gate.py | scenario | 2 | 2 | N/A-INPUT（需 `--scenario`） |
| rng_scene_gate.py | gate | 2 | 2 | N/A-INPUT（需 `--scenario/--a/--b`） |
| rng_scene_gate.py | selftest | 0 | 0 | GREEN |

> **2026-10-17 订正（来源 `6c2e5d9a2` / `e7d432189`）**：原 §3 清单编于 `a18586f39`，**未含** `rng-trace-probe/`（`rng_ledger_diff.py` / `rng_consumer_groups.py` / `analyze_rng_trace.py`）与 `attack_rng_ledger.py` 的 `selftest`；清单其余脚本**均在范围内**（含 `m3_whitelist_registry_audit.py`、`check_evidence_counts.py` / `check_evidence_head.py`、`check_window_batch.py`）。理由 清单范围遗漏，非门红。现值见 §7 末四行，均在 `e7d432189` 上实跑 rc=0。

### 3.1 两类标记

- **基线就红（既存债）**：`check_anchors.py` 的 `check` / `selftest`（**1 个根因**，本片已修）。
  这也是 `fixture_chain_gate.py check` 三条 RED 的唯一来源。
- **本轮新红**：**无**（本轮 rc 列除上述两条由 1→0 外，与基线逐条相同）。
- **N/A-INPUT（`rc=2`）**：`attack_stream_gate.py gate`、`check_evidence_counts.py check`、
  `check_evidence_head.py check/check-pack`、`rng_scene_gate.py scenario/gate` —— argparse 缺必填入参即退出，
  **不是门失败**（喂真实夹具/证据目录时由对应 runner 驱动，如 `fixture_chain_gate` 已覆盖其真身）。

## 4. fixture_chain_gate 复跑读数

| 判据 | 命令 | `a18586f39` | 本轮（修后） |
| --- | --- | --- | --- |
| 链深检 | `python -I fixture_chain_gate.py check` | `FIXTURE_CHAIN RED runner_ok=9 prereq_or_skip=0 red=3` rc=1 | `FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0` rc=0 |

（三条 RED 收敛后，`B` 系统 runner `-SelfCheckNoStack` 由 rc=1 → rc=0；共享管线
`rng_scene_anchor_check` / `rng_scene_anchor_selftest` 转 GREEN。原始件见 §6。）

## 5. 门禁（四件套 + 对账门 + 锚点门，cwd=`mir2-rs`）

见 §6 原始件；读数小结：

- `cargo fmt --all -- --check` rc=0
- `cargo clippy --workspace --all-targets -- -D warnings` rc=0
- `cargo run -q -p msgcodegen -- --check` rc=0
- `cargo test --workspace` rc=0：**104 suites / 1573 passed / 0 failed / 25 ignored**
- 对账门 `recon_gates.py {content,dispatch,selftest,mutate}` 四子命令：均 rc=0；content/dispatch 输出在**路径归一后 `diff` = IDENTICAL** ⇒ status 列与 `a18586f39` **逐行相同**
- 锚点门 `whitelist_anchor_gate.py check` rc=0、`m3_whitelist_registry_audit.py audit` rc=0

## 6. 原始件与残留物

原始输出（会话临时目录，**不入库**，可由命令再生）：

- `/e/tmp/fcg-gate-post.txt`（`fixture_chain_gate.py check` 修后读数；基线读数见会话首跑 `GATE_RC` 记录 / 下表）
- `/e/tmp/fcg-sweep-baseline.txt` / `/e/tmp/fcg-sweep-impl.txt`（清单核查 48 条·基线 / 修后）
- `/e/tmp/rc-b-*.txt` / `/e/tmp/rc-i-*.txt`（对账门四子命令两树全量输出；content/dispatch 路径归一后 `diff` = IDENTICAL）
- `/e/tmp/fcg-cargo.txt`（四件套）

残留物：`/e/tmp/impl-fcg`（本片 worktree）、`/e/tmp/fcg-baseline`（基线 detached worktree）、
`/e/tmp/fcg-target`（`CARGO_TARGET_DIR`）。均为本片专属，收尾按白名单删除。

---

## 7. 2026-10-17 追加：新增 rng 门与自检（基线 `e7d432189`）

> 本节在 `e7d432189`（master，含收窄 `6c2e5d9a2` + 方案 C `e7d432189`）上实跑；命令 cwd=`mir2-rs/tests/parity/m3`，`python -I`。

| # | 命令 | rc | 读数（`e7d432189`） |
| --- | --- | --- | --- |
| 1 | `python -I rng-trace-probe/rng_ledger_diff.py selftest` | 0 | `SELFTEST GREEN: rng_ledger_diff selftest 37/37` |
| 2 | `python -I rng-trace-probe/rng_consumer_groups.py selftest` | 0 | `SELFTEST GREEN: rng_consumer_groups selftest 20/20` |
| 3 | `python -I attack_rng_ledger.py selftest` | 0 | `SELFTEST GREEN: attack_rng_ledger selftest 51/51` |
| 4 | `python -I rng-trace-probe/analyze_rng_trace.py selftest` | 0 | `SELFTEST GREEN: analyze_rng_trace selftest 5/5` |
| 5 | `python -I rng-trace-scene/check_anchors.py check` | 0 | `GREEN: check_anchors 7/7 锚点保鲜` |
| 6 | `python -I rng-trace-scene/check_anchors.py selftest` | 0 | `SELFTEST GREEN: check_anchors selftest 17/17（含 16 条改坏必红）` |

### 7.1 `magic.rs` 取数已清零、`RUST_ATTACK_RE` 已收窄（基线 `e7d432189`）

- **事实（`6c2e5d9a2`）**：两处 `RUST_ATTACK_RE`（`rng-trace-probe/rng_ledger_diff.py:56`、`attack_rng_ledger.py:58-60`）同步删除 `|magic\.rs:` 片段，恢复逐字节同表；`e7d432189` 再补齐活栈行至 **24 个 combat.rs 行号**，并新增 `attack_rng_ledger.py:413` 的 selftest `rust-attack-re-identical-both-files`（只改单侧必红）。
- **事实（`e7d432189`，方案 C）**：`crates/world/src/magic.rs` 的 **21** 个 `random_below` 已按语义等价全部迁出 ⇒ 现 **0**；取数落 `magic_effect.rs`（`mpow`/`get_power`/`get_power13`/`get_rpow`/`power_defence_area`/`mag_turn_undead`/`mag_tamming`）与 `combat.rs`（`power_uenhancer` `:1383`、`group_melee_damage` `:1408`）。
- **事实（`e7d432189`）**：`consumer_groups.tsv` 的**文件级**死行 `magic_effect rust crates/world/src/magic\.rs:` 已删除，替换为精确行 `magic_effect rust crates/world/src/combat\.rs:(1383|1408)$`（`tsv:152`）。`magic_effect` 组行数仍为 17（删 1 + 加 1）。
- **未放宽判据**：`KNOWN_GROUPS`/`AUX_GROUPS` 未扩写；无 `known_false`/白名单改动；两处正则只删该片段、只补活栈行。

复核（cwd=`mir2-rs/tests/parity/m3`）：

```sh
grep -n "RUST_ATTACK_RE = re.compile" -A2 rng-trace-probe/rng_ledger_diff.py attack_rng_ledger.py
#   期望：两处 pattern 逐字相同 = combat\.rs:(67|103|106|109|111|250|579|587|601|606|608|609|628|630|736|758|763|771|790|792|831|936|1018|1126)
#         （24 个行号；无 |magic\.rs:）
grep -c "random_below" ../../../crates/world/src/magic.rs
#   期望：0（方案 C 后取数全部迁出）
grep -c "^magic_effect" rng-trace-probe/consumer_groups.tsv
#   期望：17（原 4 + 6c2e5d9a2 追加 13；e7d432189 把其中 1 条 magic\.rs 文件级行换成 combat\.rs 精确行，净数不变）
grep -nF 'combat\.rs:(1383|1408)' rng-trace-probe/consumer_groups.tsv
#   期望：命中 tsv:152（精确行登记；文件级 magic\.rs 数据行已无）
```