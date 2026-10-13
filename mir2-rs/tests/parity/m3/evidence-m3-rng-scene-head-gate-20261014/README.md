# evidence-m3-rng-scene-head-gate-20261014

卡：`openmir2-m3-rng-trace-scene-groups`（worktree `m3bag`，branch `wip/m3bag-anchor-teeth`）。
片：**reviewer-5 O-1 收口 —— 把 `check_evidence_head.py check-pack` 接进
`run_rng_trace_pair.ps1 -SelfCheckNoStack`**（从「文档约定 / 人工调用」升为门禁），tier=L、零 `.rs`、纯离线。

- patch base = **origin/master `266bdbd17`**（本卡离线面已闭合的 master；本片不推进任何 DoD 判据阈值）。
- 净差量全在 `mir2-rs/tests/parity/m3/**`（见 `10-boundary.txt`）；**零** `crates/**`、`bins/**`、`src/**`、
  `tests/parity/world/**`。

## 1. 为什么（reviewer-5 对本卡 head `fe0a85ae` 的 observation O-1）

原文见 `04-why-o1.txt`。要点：`check-pack`（`check_evidence_head.py` 里把「书证 head == 机判对象」
钉成 fail-closed 的子命令）此前**从未接入任何 runner / `-SelfCheckNoStack`**（全仓只有 README 引用）
⇒ 只能靠人工显式调用；工具一旦被写错/削弱，无栈自检咬不住。修前实拍
`grep check-pack run_rng_trace_pair.ps1` 无输出。

## 2. 本片改了什么（零 `.rs`）

`rng-trace-scene/run_rng_trace_pair.ps1` 的 `-SelfCheckNoStack` 新增 ㉛ 块（3 项）：

1. `evidence-head-selftest`：`check_evidence_head.py selftest` 必须 GREEN；
2. `evidence-head-pack-real-green`：四份已入库真件按各自 `--since` 锚 `check-pack` 必须 rc=0
   （缺包 / 声明读不到 / 锚不合法 ⇒ rc≠0 ⇒ **判红**，不静默跳过）；
3. `evidence-head-pack-mutation-red`：改坏必红 —— 副本包声明 head 改成不可解析 oid（`--decl-file` 钉住）
   ⇒ rc=1 且点名 `HEAD_UNRESOLVABLE`。

实现细节：改坏的临时副本放在**仓内**（`rng-trace-scene/.selftest-headpack-mut`，自检后删除）——
放在 `$env:TEMP`（`C:`）会撞上 `check_evidence_head.py` 的 `os.path.relpath` **跨盘** `ValueError`
（本片实测，非本卡同盘环境仍会复现）⇒ 用与 repo 同盘的仓内临时目录规避，并在 `finally` 兜底清理。

`rng-trace-scene/README.md` §5 / §5.4 / §6 同步：自检项数 **31 -> 34**，并把「不改自检项数（仍 31）」
改为「已接入」。

## 3. 判据读数（真跑；原始件见同目录）

| 项 | 命令（cwd = `mir2-rs/tests/parity/m3`，除非另注） | 读数 | 原始件 |
| --- | --- | --- | --- |
| 无栈自检 | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | **SELFTEST ok=34/34**，`GREEN` | `01-selfcheck-nostack.txt` |
| 改坏必红 | 副本包 head -> `ffff...` + `--decl-file` | **rc=1**，点名 `HEAD_UNRESOLVABLE`；删除副本后无残留 | `02-mutation-red.txt` |
| 真件四包 | `check-pack`（teeth / o1 / head-pack / head-ambig） | **四包全 rc=0** | `03-check-pack-real-packs.txt` |
| 离线全链 | 7 个 selftest + `check_anchors check` + `check_window_batch check` + `fixture_chain_gate check` | 全 rc=0（`fixture_chain_gate` `runner_ok=10 red=0`） | `05-offline-regression.txt` |
| 四门禁（cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/openmir2-m3bag-target`） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / 604 rc=0 | `06`-`09` |
| 边界 | `git diff --name-only <base>..<head>` | 全在 `mir2-rs/tests/parity/m3/**`；**零 `.rs`**、冻结面空 | `10-boundary.txt` |
| 本包 check-pack | `check-pack --dir evidence-m3-rng-scene-head-gate-20261014 --since 266bdbd17` | rc=0 | `11-self-check.txt` |
| 证据 hash / EOL | `verify_sha256sums.py verify --check-extra` / `eolcheck` | GREEN（仓库 LF/git-blob 口径） | `SHA256SUMS.txt` |

> 说明：本 README **刻意不硬编码「件数 / GREEN x/y」声明**（真源 = `SHA256SUMS.txt` 与目录），
> 避免重犯 `check_evidence_counts` 咬的那族「交付后证据包长大 ⇒ 声明过期」。

## 4. 未收 / 如实登记

- 本片是 **L 级离线夹具/工具片**，**不改**任何 DoD 阈值（门、组集合、折叠口径、阈值全不动）；
- 卡级真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` 的 site/order diff=0）仍
  **R / not_verified**，阻塞 = 承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口；
  **本片不得据此收卡**（卡保持 in-progress）。
- 不把本包自身加进 runner 的 4 包列表：包内声明的「读数 head = 本片交付 commit」，若自引用会形成
  「commit 哈希依赖包内容、包内容依赖 commit 哈希」的自指死结 ⇒ runner 列表只列**已合入**的四份真件
  （稳定锚），本包用第 6 条单跑自证。

## 5. 纪律 / 边界

- **未 merge、未自签 review**；只在自己 worktree 写；隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3bag-target`。
- **零起栈 / 零占端口**（十端口实拍：仅共享 3306）；共享主检出零改动。
- 未动 `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`、drop-exp 实现。
