# evidence-m3-chain-gate-o1o2-closeout-20261014 —— 链深检门 O-1/O-2 残余收口（证据包）

- 卡：`openmir2-m3-fixture-chain-gate`
- 收口对象：已合入 master `ffbaaa72e` 的 `../evidence-m3-chain-gate-o1-retro-20261014/`（被审 head `f58b8dfa6`）上，
  reviewer-5（approve @reviewed_head `f58b8dfa6`）携带的两条**非阻塞 Minor**：**O-1 / O-2**。
  ⇒ 本片是该已合片上的**独立后续小片**（分支 `worktree-m3-chain-gate-o1o2-closeout`）。
- base：`origin/master` = `3db79cae0`（交付 patch 声明 base）。本片树上 `src/`、`mir2-rs/crates/`、
  `mir2-rs/bins/`、`MirClient/`、`sql/` 与当前 `origin/master` 逐字节相同（`14-boundary.txt`）。
- head / branch：`worktree-m3-chain-gate-o1o2-closeout`（自 `origin/master` `3db79cae0` 起，不改写已合片历史）
- 作者：`openmir2-worker-5`；**待独立 principal 复核**（作者不自签）
- tier：**L**（纯文档/证据：零 `.rs`、零 `.toml`、零 `.ps1`、零 `.cs`，零运行时；
  不触 RNG 抽数序 / 时序 / 落库 / 战斗数值）；`window_required = false`
- 起栈 0 / 端口 0；未 merge；只在 `m3cs` worktree 写/提交。

## 1. 做了什么

| # | 原文（reviewer-5 @`f58b8dfa6`） | 处置 | 件 |
| --- | --- | --- | --- |
| **O-1**（Minor·文档时效） | 新证据包 README 写 base / 链门树为 `515c6b5ea`，而交付 patch 的 base 是 `f43364c45`（此后两次 `merge origin/master`） | 订正 `../evidence-m3-chain-gate-o1-retro-20261014/README.md` 三处（头部 base、§2 链门树、§3 四门禁）为 `f43364c45`，各留「初稿编写时为 `515c6b5ea`」追记 | `01` |
| **O-2**（Minor·证据卫生） | `14-redcheck-readme-drift.txt` 含作者 worktree 绝对路径，且留下临时件 `README.md.o1bak`（RED 行「多件: README.md.o1bak」） | 重抓该件：备份件放**仓外** ⇒ 无 `.o1bak`/「多件」行；路径归一为 `<repo>`；牙齿结论不变（RED 15/16 点名 `README.md` → 还原 GREEN 16/16）。原 `14` sha256 `7695d46a…` 在 README §5 留档 | `02` |

净差量（`01`）：该包 `14-redcheck-readme-drift.txt`（重抓）、`README.md`（+§5 收口记录）、`SHA256SUMS.txt`；
包内 `SHA256SUMS.txt` 按**仓库口径（LF 归一 = git blob）** 重记：被改件 = `README.md` / `14-*`；重记后幂等复跑 = **0 行变更**（`04`）。

## 2. 读数与牙齿

| 项 | 读数 | 件 |
| --- | --- | --- |
| O-1 订正 | `git diff origin/master` 三处 `515c6b5ea` → `f43364c45`（其余不变） | `01` |
| **O-2 改坏必红（重抓）** | 追改 `evidence-m3-chain-gate-residuals` 的 README **不重记**清单 ⇒ `verify --check-extra` **RED 15/16** 并点名 `README.md`（清单 `77fcfec5…` vs 实算 `40444f19…`）rc=1；按 sha256 还原 ⇒ **GREEN 16/16** rc=0；`git status --porcelain` 空（无临时残留） | `02` |
| 本包（o1-retro）复核 | `verify --check-extra` **OK 17/17** rc=0；`eolcheck` **GREEN 18/18** rc=0 | `03` |
| 清单重记 | 被改件 = `README.md`/`14-*` 两件；重记后幂等复跑 = 0 行变更 | `04` |
| 链门 selftest | `SELFTEST GREEN: fixture_chain_gate selftest 40/40` rc=0 | `05` |
| 链门 check | `FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0` rc=0（含 `fixture_fmt_evidence_check GREEN（9/9）` + `rng_scene_anchor_*` GREEN） | `06` |
| fmt 证据门 | `SELFTEST GREEN 8/8` · `FIXTURE_FMT_EVIDENCE GREEN（9 包 / 9 件）` · `MUTATION GREEN 5/5`，均 rc=0 | `07`–`09` |

> O-2 重抓件（`02`）里出现的 `EOL 陷阱:` 段是 `verify_sha256sums.py` 的**改坏窗口期**提示（清单 hash 恰好等于
> HEAD blob，而工作树 README 已被追加 ⇒ 工具按「清单 == HEAD blob」列出可能的行尾陷阱），**不是 EOL 缺陷**：
> 还原后 `GREEN 16/16`，且本包 `eolcheck 18/18` GREEN（`03`）。上一片原版 `14` 未触发该提示，只因当时该 README
> 尚未提交（HEAD 无该 blob）。

## 3. 四门禁（cwd = `mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`）

| 门 | 读数 | 件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `10` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `11` |
| `cargo test --workspace` | 101 suites / 1219 passed / 0 failed / 22 ignored，rc=0 | `12` |
| `cargo run -p msgcodegen -- --check` | 604 条一致，rc=0 | `13` |

> 本片零 `.rs`/`.toml`/`.cs` ⇒ 四门禁读数与 base 一致，此处为**本片树上实测**。

## 4. 边界与残留

- 边界（`14`）：自 `origin/master`（`3db79cae0`）的差量 = 3 路径（M，均在本线已合片 `evidence-m3-chain-gate-o1-retro-20261014/`）
  + 本证据包 18 件（A），全部前缀 `mir2-rs/tests/parity/m3/evidence-m3-chain-gate-*`；
  `*.rs=0`、`*.toml=0`、`*.ps1=0`、`*.cs=0`；冻结面 `src/ sql/ MirClient/ mir2-rs/crates/protocol/
  mir2-rs/crates/storage/ mir2-rs/bins/ src/BotSrv/` 全 0。
- 本片**只做文档/证据收口**，不改任何 gating 逻辑（`fixture_chain_gate.py` / `m3_fixture_fmt_evidence_gate.py`
  零改动）。
- 残留（如实登记，**不在本片写集**）：全仓另有 ≈40 个历史 `*fmt*` 欠件、`whitelist.md` T-42 撞号
  （⑤-2 world 行 vs ⑥-5 m3 行），均需 owner 路由（前者需单开卡，后者涉及 `tests/parity/world/**` 写集）。
- 卡级真机 DoD（各 READY 夹具 `run_*.ps1` 两侧同序列 `diff=0`）仍等世界侧实现 +
  `openmir2-m3-clock-injection` + owner 独占窗口，本片不据此收卡。
