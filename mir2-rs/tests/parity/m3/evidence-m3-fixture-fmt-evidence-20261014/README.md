# evidence-m3-fixture-fmt-evidence-20261014 — 夹具链族「fmt 证据自描述」收口

- 卡：`openmir2-m3-fixture-chain-gate`
- 收口项：reviewer-6 `090d0cc4`（@reviewed_head `f114a5e90`）**O-1**（非阻塞）；
  同族同病另 4 件（见下）
- base：origin/master `f0a9ab911`（合并后的树）
- head / branch：`worktree-m3-chain-gate-fmt-evidence`
- 作者：`openmir2-worker-5`；**待独立 principal 复核**
- tier：**L**（纯证据/工具/文档，零 `.rs`、零运行时、冻结面空；不触 RNG 抽数序 / 时序 / 落库 / 战斗数值）

## 1. 收口项（reviewer-6 O-1）

> 「evidence 包 40-fmt.txt 为 0 字节空文件，且未按本包其余证据的 `$ <命令> / [rc=0]` 体例记录
> 命令头与退出码 ⇒ 该件本身不能证明 fmt 绿。……建议下次动该包时补命令头+rc 标记。」

排查发现这不是孤例：夹具链族 9 个证据包里共 **5 件** `*fmt*.txt` 无法自证（`01-before-scan.txt`）：

| 路径 | 收口前 | 收口后 |
| --- | --- | --- |
| `evidence-m3-anchor-prefix-o2-20261014/40-fmt.txt` | 0 B（空） | `$ cargo fmt --all -- --check` + `rc=0` |
| `evidence-m3-chain-gate-o2-20261014/20-gate-fmt.txt` | 0 B（空） | 同上 |
| `evidence-m3-fixture-chain-hero-anchor-20261014/05-gate-fmt.txt` | 1 B（仅换行） | 同上 |
| `evidence-m3-fixture-family-minors-20261014/08-gate-fmt.txt` | 0 B（空） | 同上 |
| `evidence-m3-chain-gate-residuals-20261014/10-gate-fmt.txt` | 5 B（仅 `rc=0`，无命令头） | 同上 |

5 个证据包的 `SHA256SUMS.txt` 按**仓库口径（LF 归一 = git blob 字节）**重记并复核
（`verify --check-extra` GREEN + `eolcheck` GREEN，见 `20-evidence-verify.txt`）。

## 2. 防复发：`m3_fixture_fmt_evidence_gate.py`（新门，有牙）

只读、零端口、不依赖 cargo/dotnet。规则：登记 9 个夹具链族证据包，每包**至少 1 件** `*fmt*.txt`，
且每件必须 **非空 + 含 `$ <命令>` 命令头 + 含 `rc=<n>`**；否则点名 fail-closed（`rc=1`）。

- `check`：真身扫描 ⇒ `FIXTURE_FMT_EVIDENCE GREEN（9 个证据包 / 9 件）`（`02`）
- `selftest`：8/8 条牙（合规 GREEN + 空件 / 纯空白 / 无命令头 / 无 rc / 无 fmt 件 / 包缺失 各 RED；
  非 `*fmt*.txt` 的 raw 证据不误伤）（`03`）
- `mutation`：临时副本 MUT1 抹空 / MUT2 删命令头 / MUT3 删 rc / MUT4 删件，全 RED（`04`）
- 外部改坏（不复用作者 RC）：真身抹空 ⇒ `check rc=1`，还原 ⇒ `rc=0`（`21`）

门已接入 `fixture_chain_gate.py` 的 `SHARED_CHECKS`
（`fixture_fmt_evidence_selftest` + `fixture_fmt_evidence_check`），不看矩阵状态、独立常跑；
`fixture_chain_gate.selftest` 内含**防回退牙**（本门被反注册 ⇒ 链门 selftest RED），
并随合并把「主机门（.py）登记项」下界订正为 ≥21（含 ⑧-5 行会战怪行为登记门）。

## 3. 读数（全在合并 origin/master `f0a9ab911` 后的树上）

| 项 | 读数 | 件 |
| --- | --- | --- |
| 新门 check | `FIXTURE_FMT_EVIDENCE GREEN（9/9 件）` | `02` |
| 新门 selftest | `SELFTEST GREEN 8/8` | `03` |
| 新门 mutation | `MUTATION GREEN 5/5`（含真身体例 GREEN） | `04` |
| 链门 selftest | `SELFTEST GREEN: fixture_chain_gate selftest 39/39` | `05` |
| 链门 check | `fixture_fmt_evidence_selftest/check rc=0`；`FIXTURE_CHAIN RED runner_ok=9 red=1` | `06` |
| 5 包 hash | `verify --check-extra` OK 27/27 · 18/18 · 16/16 · 12/12 · 14/14；`eolcheck` 全 GREEN | `20` |
| 四门禁 | fmt 0 / clippy 0(-D warnings) / 101 suites 1213 passed 0 failed 22 ignored / msgcodegen 604 | `10`–`13` |

## 4. 链门 FIXTURE_CHAIN 的 RED 归因（不假绿）

`fixture_chain_gate.py check` 仍 `RED red=1`，唯一点名项为 **B 全服 RNG 消费者账本（Claim B）**
`runner -SelfCheckNoStack rc=1（SELFTEST ok=36/37 failures=anchors-freshness-selftest）` ——
在**纯 base（未含本片改动）**的临时 detached worktree 上同项同 RED
（`09-chain-red-preexisting-base-proof.txt`）⇒ 属 Claim B 承接卡遗留
（`openmir2-m3-rng-trace-scene-groups` / `openmir2-m3-rng-scene-driver` 未开 + 窗口），
**与本片写集（fmt 证据）无关**，本片不据此判红、也不修。

## 5. 边界与残留

- 边界：仅 `mir2-rs/tests/parity/m3/**`（5 个证据包 + 新门 + 链门接入 + 本证据包）；
  `*.rs=0`、`*.toml=0`、`*.ps1=0`；冻结面 `src/ sql/ MirClient/ mir2-rs/crates/protocol/
  mir2-rs/crates/storage/ mir2-rs/bins/ src/BotSrv/` 空（`07-boundary.txt`）。
- 残留（如实登记，**不在本片写集**）：全仓另有 ≈40 个历史 `*fmt*` 证据件同样 0 字节/无命令头
  （如 `evidence-attack-*-20261012/*gates-fmt.txt`、`evidence-*/gates/10-fmt.txt` 等）。
  本门只覆盖夹具链族 9 包；全仓证据卫生清理需单开卡 + 逐包重记 `SHA256SUMS`，本片不越界。
- 卡级真机 DoD（各夹具两侧同序列 diff=0）仍等 owner 独占窗口，本片不据此收卡。
- 起栈 0 / 端口 0；未自签 review；未 merge。
