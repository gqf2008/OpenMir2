# evidence-m3-chain-gate-o1-retro-20261014 —— 链深检门 O-1 残余：5 个被追改证据包的「包内登记」

- 卡：`openmir2-m3-fixture-chain-gate`
- 收口项：reviewer-6 `090d0cc4`（@reviewed_head `f114a5e90`）**O-1** 的残余建议
  ——「这些包在 merge 时的字节与当前 head 不同，建议（**下次动这些文件时**）在**包内 README 或
  whitelist** 追一行『追改原因+日期』，免得日后审计把该漂移误当篡改」。
- base：`origin/master` = `515c6b5ea`
- head / branch：`worktree-m3-chain-gate-o1-retro`
- 作者：`openmir2-worker-5`；**待独立 principal 复核**
- tier：**L**（纯文档/证据片：零 `.rs`、零 `.toml`、零 `.ps1`、零 `.cs`，零运行时；
  不触 RNG 抽数序 / 时序 / 落库 / 战斗数值）

## 1. 做了什么

5 个被追改包（名单同 `../evidence-m3-fixture-fmt-evidence-20261014/README.md` §1）各在
`README.md` 末尾追加一段「追改登记（retro-fix record，2026-10-14）」，写明：

- 被追改的 `*fmt*.txt` **原状**（0 B / 1 B / 仅 `rc=0`）；
- 追改**原因**（O-1：原件不能自证 fmt 绿）与**改后体例**（`$ cargo fmt --all -- --check` + `rc=0`）；
- 口径（`SHA256SUMS.txt` 按**仓库字节（LF 归一 = git blob）**重记）；
- 定性（**方向为增强、非篡改**）与 5 包清单指针。

| 包 | 追改件 | 原状 | 追记段 |
| --- | --- | --- | --- |
| `evidence-m3-anchor-prefix-o2-20261014` | `40-fmt.txt` | 0 B（空） | §「追改登记」 |
| `evidence-m3-chain-gate-o2-20261014` | `20-gate-fmt.txt` | 0 B（空） | 同上 |
| `evidence-m3-chain-gate-residuals-20261014` | `10-gate-fmt.txt` | 5 B（仅 `rc=0`，无命令头） | 同上 |
| `evidence-m3-fixture-chain-hero-anchor-20261014` | `05-gate-fmt.txt` | 1 B（仅换行） | 同上 |
| `evidence-m3-fixture-family-minors-20261014` | `08-gate-fmt.txt` | 0 B（空） | 同上 |

每包 `SHA256SUMS.txt` 同步重记：**恰 1 行 hash 变更**（`README.md`），其余 27/18/16/12/14 行逐字节不变
（`02-hash-regen.txt`：首次 write = 1 行变更/包；幂等复跑 = 0 行变更/包）。

## 2. 读数与牙齿

| 项 | 读数 | 件 |
| --- | --- | --- |
| 5 包复核 | `verify --check-extra` OK 27/27 · 18/18 · 16/16 · 12/12 · 14/14；`eolcheck` 28/28 · 19/19 · 17/17 · 13/13 · 15/15 全 GREEN | `03` |
| **改坏必红** | 给 `chain-gate-residuals` 的 `README.md` 追改**不重记**清单 ⇒ `verify --check-extra` **RED 15/16** 并点名 `README.md`（清单 `77fcfec5…` vs 实算 `b6292032…`）rc=1；按 sha256 还原后 **GREEN 16/16** rc=0 | `14` |
| 本包自复核 | `verify --check-extra` OK 17/17；`eolcheck` 18/18 GREEN | `15` |
| 链门 selftest | `SELFTEST GREEN: fixture_chain_gate selftest 40/40` rc=0 | `04` |
| 链门 check | `FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0` rc=0（含 `fixture_fmt_evidence_check GREEN（9 个证据包 / 9 件）`） | `05` |
| fmt 证据门 selftest/check/mutation | `SELFTEST GREEN 8/8` · `FIXTURE_FMT_EVIDENCE GREEN（9/9）` · `MUTATION GREEN 5/5` 均 rc=0 | `06`–`08` |

> 链门 check 在**当前 master `515c6b5ea` 的树**上 GREEN（本片 base）；上一 tick（`9edfc745d`）那条
> `FIXTURE_CHAIN RED`（RNG 行号锚漂移）已由 m3bag 的根因片
> `wip/m3bag-anchor-teeth`（撤行号锚改「符号存在且唯一」机判）合入 master 后自动消失 —— 本片不涉。

## 3. 四门禁（cwd = `mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`）

| 门 | 读数 | 件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `09` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `10` |
| `cargo test --workspace` | 101 suites / 1219 passed / 0 failed / 22 ignored，rc=0 | `11` |
| `cargo run -p msgcodegen -- --check` | 604 条一致，rc=0 | `12` |

> 本片零 `.rs`/`.toml`/`.cs` ⇒ 四门禁读数预期与 base `515c6b5ea` 一致，此处为**本片树上实测**。

## 4. 边界与残留

- 边界：仅 `mir2-rs/tests/parity/m3/**`（5 个被追改包的 `README.md`+`SHA256SUMS.txt` + 本证据包）；
  `*.rs=0`、`*.toml=0`、`*.ps1=0`、`*.cs=0`；冻结面 `src/ sql/ MirClient/ mir2-rs/crates/protocol/
  mir2-rs/crates/storage/ mir2-rs/bins/ src/BotSrv/` 空（`13-boundary.txt`）。
- 本片**只补登记**，不改任何 fmt 证据内容，也不动 gating 逻辑（`fixture_chain_gate.py` /
  `m3_fixture_fmt_evidence_gate.py` 零改动）。
- 残留（如实登记，**不在本片写集**）：全仓另有 ≈40 个历史 `*fmt*` 欠件（`m3_fixture_fmt_evidence_gate`
  的 `not_done` 已登记）；全仓证据卫生清理需单开卡。
- 卡级真机 DoD（各 READY 夹具 `run_*.ps1` 两侧同序列 `diff=0`）仍等世界侧实现 +
  `openmir2-m3-clock-injection` + owner 独占窗口，本片不据此收卡。
- 起栈 0 / 端口 0；未自签 review；未 merge；只在 `m3cs` worktree 写/提交。
