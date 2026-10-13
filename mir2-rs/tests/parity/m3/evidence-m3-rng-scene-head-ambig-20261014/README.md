# evidence-m3-rng-scene-head-ambig-20261014

卡：`openmir2-m3-rng-trace-scene-groups`（worktree `m3bag`，branch `wip/m3bag-anchor-teeth`）。
片：**reviewer-6 O-1 收口 —— `check_evidence_head.py check-pack` 对「同一包多处 `head = <oid>` 声明」
不再静默取首个**，tier=L、零 `.rs`、纯离线。

- patch base = **origin/master `b00987707`**（本卡上一片 `fe0a85aee` 已合入 master `b00987707`）。
- 净差量全在 `mir2-rs/tests/parity/m3/**`（见 `10-boundary.txt`）；**零** `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`。

## 1. 为什么（reviewer-6 对 head `fe0a85aee` 的 observation O-1）

见 `01-observation-o1.txt`（原文）。要点：`check-pack` 旧口径在缺省探测里**只取首个**声明；
若同一包同时含「读数 head」与「交付 head」两处声明，会**静默**采信先出现者
（缺省顺序 `11-boundary.txt` -> `10-boundary.txt` -> `README.md`）⇒ 书证与机判对象可能错位而不自知。

## 2. 本片改了什么（零 `.rs`）

`rng-trace-scene/check_evidence_head.py` 的 `_read_declared_head` 由「找到首个即返回」改为
**收集全部声明**；`check_pack` 先把每条声明 `git rev-parse` **归一到完整 oid**，再判：

- 全部声明指向**同一 commit**（哪怕写成不同长度前缀，真件 `teeth` 包即 `2591a7b31` vs `2591a7b316ae`）
  ⇒ 采信首个（来源 / 行号 / 声明处数记入 `decl=`）并继续；
- 出现**多处不同 commit** ⇒ `PREREQ_AMBIGUOUS_HEAD_DECL`（rc=2）逐处点名 `[oid[:12]] raw@file:line`，
  要求用 `--decl-file` 钉住应采信的那一处。

其余四类判据（可解析 / since 祖先 / HEAD 祖先 / 仅证据侧）不变。
`selftest` **16/16 -> 19/19**（+ 歧义必红 / 同 commit 异前缀绿 / `--decl-file` 钉住绿）；
`rng-trace-scene/README.md` §5.4/§6 同步降级旧计数并写入本口径。

## 3. 判据读数（真跑；原始件见同目录）

| 项 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 | 原始件 |
| --- | --- | --- | --- |
| 工具自检 | `check_evidence_head.py selftest` | **19/19 GREEN**（含改坏必红 + PREREQ） | `02-gate-selftest.txt` |
| 真件三包 | `check-pack`（teeth / o1 / head-pack） | **三包全 rc=0**；teeth 命中「共 2 处声明」但归一后同 commit ⇒ 不判歧义 | `03-check-pack-green.txt` |
| 改坏必红 | 把歧义门短路（`if len(groups) > 1` -> `if False`） | `SELFTEST_RED 18/19`（`pack-ambiguous-decl-prereq` FAIL，rc=1）⇒ 还原 sha256 一致、复绿 19/19 | `04-mutation-red-first-wins.txt` |
| 离线全链 | 7 个 selftest + `check_anchors check` + `check_window_batch check` + `fixture_chain_gate selftest` | 全 rc=0（`fixture_chain_gate` 31/31） | `05-offline-regression.txt` |
| 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`，cwd=`mir2-rs`） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / 604 rc=0 | `06`-`09` |
| 边界 | `git diff --name-only <base>..HEAD` | 全在 `mir2-rs/tests/parity/m3/**`；**零 `.rs`**、冻结面空 | `10-boundary.txt` |
| 证据 hash / EOL | `verify_sha256sums.py verify --check-extra` / `eolcheck` | GREEN / GREEN（仓库 LF/git-blob 口径） | `SHA256SUMS.txt` |

## 4. O-2 / 未收的口径（如实登记）

- reviewer-6 的 **O-2** 是「件数口径小差」（清单行数 vs 目录件数；20 vs 19 路径），属**人写书证**，
  无代码面可机判 ⇒ 本片不为其造门；本 README **刻意不写「件数 / GREEN x/y」声明**，避免重犯该族。
- 本门只把「多处声明**取值不同**」判红；同一 commit 的不同前缀写法（真件 `teeth` 包）归一后放行，
  既收住 O-1 的静默首取，又**不误伤**合规包。

## 5. 纪律 / 边界

- **未 merge、未自签 review**；只在自己 worktree 写；隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-target`。
- **零起栈 / 零占端口**（跑门禁前实拍十端口：仅共享 3306）；共享主检出零改动。
- 未动 `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`、drop-exp 实现。
- 卡级真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` 的 site/order diff=0）仍
  **R / not_verified**，阻塞 = 承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口；
  本片**不得**据此收卡。
