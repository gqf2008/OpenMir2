# evidence-m3-rng-anchor-fresh-repin-20261014 — M3 夹具链 RED 收口：RNG 场景锚门行号再漂移（5665→5795）

- 卡：`openmir2-m3-fixture-chain-gate`（本 head = 上一片 `becf075db`（fmt 证据自描述收口）
  与本次锚点订正的合并树；patch `base` 已对齐最新 master）
- 触发：master `8f58bded9`（随后 `41e61029e`）上 `python -I fixture_chain_gate.py check` 为
  `FIXTURE_CHAIN RED: runner_ok=9 prereq_or_skip=0 red=3`，**三红同根**：
  - `B 全服 RNG 消费者账本（Claim B）` runner `-SelfCheckNoStack` rc=1（`anchors-freshness-selftest`，36/37）
  - 共享管线 `rng_scene_anchor_selftest` rc=1（`SELFTEST RED: check_anchors selftest 12/13`）
  - 共享管线 `rng_scene_anchor_check` rc=1（`RED: check_anchors 1 项 RED`）
- 根因（机判，`01`）：承接卡规格 `mir2-rs/tests/parity/m3/rng-trace-scene/DRIVER-CONTRACT.md:59`
  写 `main.rs:5665`，但**合并目标 master** 上 `enable_production_monster_ai(&mut world)`
  的首个匹配已是 `:5795`。m3bag 在**分支上**核出的 `5665` 在 merge 目标上已失准
  （master 侧 ⑦-`gotolabel-ext`（`17389f05f`）把 `main.rs` 加长）⇒ 正是该门 README §5.2.3
  预告的「合入后继续漂移」实例（`README.md` 本次补记）。
- 收口（零 `.rs`、零行为改动）：`DRIVER-CONTRACT.md:59` 由 `:5665` 订正为 `:5795`，
  并在 `rng-trace-scene/README.md` §5.2.3 记录本实例与经验（行号类锚点须在**合并树上**重钉）。
- 读数：`check_anchors check` **7/7 GREEN**（`02`）、`selftest` **13/13**（`03`）；
  `FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0`（`04`）；
  链门 `selftest 40/40`（`05`）；外部改坏必红（`:5795`→`:5665` ⇒ check RED + selftest 12/13，
  还原 ⇒ GREEN 且 `git diff` 空，`06`）；四门禁 fmt/clippy/test/msgcodegen（`10`–`13`）。
- base / head：origin/master `41e61029e` / `224cea0c9`（+ 本证据提交）；
  branch `worktree-m3-chain-gate-fmt-evidence`；双端 push origin=walgit（本片后续）。
- 作者 / 复核：`openmir2-worker-5` / **待独立 principal**（不 merge、不自签）。
- tier：**L**（文档行号 + 证据；零 `.rs`、零运行时、冻结面空；不触 RNG 抽数序 / 时序 / 落库 / 战斗数值）。
- 边界（`07`）：`git diff origin/master HEAD` = 32 路径，全部 `mir2-rs/tests/parity/**`；
  `*.rs` = 0；冻结面（`src/ sql/ MirClient/ mir2-rs/crates/protocol/ mir2-rs/crates/storage/ mir2-rs/bins/`）= 0。

- **写集声明（如实登记，不藏）**：本片改的 `rng-trace-scene/DRIVER-CONTRACT.md` 与
  `rng-trace-scene/README.md` 也落在 `openmir2-m3-rng-trace-scene-groups`（owner=worker-4/m3bag）
  的写集里。该卡本 tick 已收工、剩余 DoD 阻塞于**未开卡的承接卡 `openmir2-m3-rng-scene-driver` +
  独占窗口**（无并发写者）；本改动是**行号订正**（非语义、非门阈值）。若 owner 判定越界：
  撤掉 `DRIVER-CONTRACT.md` 的 1 行订正即可（`FIXTURE_CHAIN` 随之回 RED），其余证据保留。

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 本片完整命令清单（可复跑） |
| `01-anchor-before-master-red.txt` | `origin/master` 的 doc（`:5665`）在 `--doc` 副本上 ⇒ `ANCHOR_DRIFT` RED（rc=1） |
| `02-anchor-after-green.txt` | 收口后 `check_anchors.py check` ⇒ GREEN 7/7 |
| `03-anchor-selftest.txt` | `check_anchors.py selftest` ⇒ 13/13（含 12 条改坏必红） |
| `04-chain-gate-green.txt` | `fixture_chain_gate.py check` 全量 ⇒ `FIXTURE_CHAIN GREEN runner_ok=10 red=0` |
| `05-chain-gate-selftest.txt` | `fixture_chain_gate.py selftest` ⇒ 40/40（含并集后的两条防回退牙） |
| `06-mutation-doc-drift-red.txt` | 外部改坏必红：`:5795`→`:5665` ⇒ check RED + selftest 12/13；还原 ⇒ GREEN 且 `git diff` 空 |
| `07-boundary.txt` | 边界：32 路径全在 `mir2-rs/tests/parity/**`、`*.rs`=0、冻结面 0、`git status` 干净 |
| `10-gate-fmt.txt` / `11-gate-clippy.txt` / `12-gate-test.txt` / `13-gate-msgcodegen.txt` | 四门禁读数（HEAD `224cea0c9`，隔离 `CARGO_TARGET_DIR`） |
| `20-evidence-verify.txt` | 本包 `SHA256SUMS` 的 `verify --check-extra` + `eolcheck`（仓库 LF/git-blob 口径） |
| `21-doc-consistency.txt` | 文档行号 / `ANCHORS` 登记 / 源码实际行的三点一致对照 |
| `SHA256SUMS.txt` | 本包全部文本件的仓库口径 hash 清单 |

## 口径说明（与同族的差异，避免误解）

- 本门（`m3_fixture_fmt_evidence_gate.py`）只登记**夹具链族 9 个历史证据包**；本包不在其
  `FAMILY_DIRS` 里（该门自述：全仓 ≈40 个历史 `*fmt*` 欠件的卫生清理需单开卡）。本包
  `10-gate-fmt.txt` 仍按同族体例（`$ <命令>` + `rc=0`）书写。
